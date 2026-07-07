use crate::error::Result;
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

/// Cap for files considered for path rewriting. Larger gitignored
/// artifacts (build output, caches, large `.env`) are skipped without
/// being read into memory.
const MAX_REWRITE_FILE_BYTES: u64 = 2 * 1024 * 1024;

pub struct PathRewriter {
    src_path: PathBuf,
    dest_path: PathBuf,
}

impl PathRewriter {
    pub fn new<P: AsRef<Path>, Q: AsRef<Path>>(src_path: P, dest_path: Q) -> Self {
        Self {
            src_path: src_path.as_ref().to_path_buf(),
            dest_path: dest_path.as_ref().to_path_buf(),
        }
    }

    /// Rewrite absolute paths across the whole destination worktree.
    ///
    /// The `warp` bin scopes rewriting to the overlaid entries via
    /// [`Self::rewrite_paths_under`]; this whole-tree variant remains for the
    /// library's tests and benchmarks.
    #[allow(dead_code)]
    pub fn rewrite_paths(&self) -> Result<()> {
        let dest_path = self.dest_path.clone();
        self.rewrite_files(collect_files(&dest_path))
    }

    /// Rewrite absolute paths only within `roots` (files or directories),
    /// instead of walking the entire worktree.
    ///
    /// The CoW overlay copies just the untracked/ignored entries, so only those
    /// can carry a baked-in old path. Scoping to them keeps the walk small and,
    /// critically, never rewrites tracked files — which git already checked out
    /// and which must not gain spurious diffs.
    pub fn rewrite_paths_under(&self, roots: &[PathBuf]) -> Result<()> {
        let files: Vec<PathBuf> = roots.iter().flat_map(|root| collect_files(root)).collect();
        self.rewrite_files(files)
    }

    fn rewrite_files(&self, files: Vec<PathBuf>) -> Result<()> {
        let src_str = self.src_path.to_string_lossy();
        let dest_str = self.dest_path.to_string_lossy();

        // Process files in parallel
        files.par_iter().for_each(|file_path| {
            if let Err(e) = self.rewrite_file(file_path, &src_str, &dest_str) {
                log::warn!("Failed to rewrite paths in {}: {}", file_path.display(), e);
            }
        });

        Ok(())
    }

    /// Rewrite paths in a single file
    fn rewrite_file(&self, file_path: &Path, src_str: &str, dest_str: &str) -> Result<()> {
        // Skip files larger than the rewrite cap before loading them.
        match fs::metadata(file_path) {
            Ok(meta) if meta.len() > MAX_REWRITE_FILE_BYTES => {
                log::debug!(
                    "Skipping path rewrite for {} ({} bytes exceeds cap)",
                    file_path.display(),
                    meta.len()
                );
                return Ok(());
            }
            Ok(_) => {}
            Err(_) => return Ok(()),
        }

        // Read file content
        let content = match fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(_) => {
                // Skip binary files or files we can't read as UTF-8
                return Ok(());
            }
        };

        // Check if file contains the source path
        if !content.contains(src_str) {
            return Ok(());
        }

        // Skip files that are likely binary
        if self.is_likely_binary(&content) {
            return Ok(());
        }

        // Replace paths only at path boundaries to avoid corrupting
        // unrelated paths that share the source as a prefix.
        let new_content = replace_at_path_boundary(&content, src_str, dest_str);

        // Write back if content changed
        if new_content != content {
            crate::fs_atomic::write_atomic(file_path, new_content.as_bytes())?;
            log::debug!("Rewrote paths in: {}", file_path.display());
        }

        Ok(())
    }

    /// Simple heuristic to detect binary files
    fn is_likely_binary(&self, content: &str) -> bool {
        // Check for null bytes (common in binary files)
        content.contains('\0') ||
        // Check for very high ratio of non-printable characters
        {
            let total = content.chars().count();
            if total == 0 {
                return false;
            }

            let printable = content
                .chars()
                .filter(|c| !c.is_control() || c.is_whitespace())
                .count();

            let printable_ratio = printable as f64 / total as f64;
            printable_ratio < 0.95
        }
    }
}

/// Collect the regular files under `root` for path rewriting.
///
/// The rewriter targets gitignored artifacts (venvs, build output, generated
/// configs) that bake in the old worktree path, so the walker must NOT filter
/// on gitignore. The `.git` entry is pruned explicitly: in a primary worktree
/// it is the repo's internal directory, and in a secondary worktree it is a
/// `gitdir:` pointer file that git owns. A `root` that is itself a file yields
/// just that file.
fn collect_files(root: &Path) -> Vec<PathBuf> {
    WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .require_git(false)
        .filter_entry(|entry| entry.file_name() != ".git")
        .build()
        .filter_map(|entry| match entry {
            Ok(entry) => {
                if entry.file_type()?.is_file() {
                    Some(entry.path().to_path_buf())
                } else {
                    None
                }
            }
            Err(_) => None,
        })
        .collect()
}

/// True when `c` can continue an absolute path token, so a match that
/// ends right before `c` is actually a prefix of a longer, unrelated
/// path and must not be rewritten.
fn is_path_continuation_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// Replace every occurrence of `src` in `content` with `dest`, but only
/// when the match is followed by a path boundary (end-of-string or any
/// char that is not `[A-Za-z0-9._-]`). Prefix collisions like
/// `/foo/repo-archive` are left intact when the source is `/foo/repo`.
fn replace_at_path_boundary(content: &str, src: &str, dest: &str) -> String {
    if src.is_empty() {
        return content.to_string();
    }

    let mut result = String::with_capacity(content.len());
    let mut last_end = 0;
    for (start, _) in content.match_indices(src) {
        if start < last_end {
            // Skip overlapping matches that fall inside a span we already
            // emitted (possible if `src` itself self-overlaps).
            continue;
        }
        let after = start + src.len();
        let next_is_boundary = match content[after..].chars().next() {
            None => true,
            Some(c) => !is_path_continuation_char(c),
        };
        if next_is_boundary {
            result.push_str(&content[last_end..start]);
            result.push_str(dest);
            last_end = after;
        }
    }
    result.push_str(&content[last_end..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    #[test]
    fn test_path_rewriting() {
        let temp_dir = tempdir().unwrap();
        let src_dir = temp_dir.path().join("src");
        let dest_dir = temp_dir.path().join("dest");

        // Create source and destination directories
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dest_dir).unwrap();

        // Create a test file with absolute paths
        let test_content = format!("export PATH=\"{}:$PATH\"", src_dir.display());
        fs::write(dest_dir.join("activate.sh"), &test_content).unwrap();

        // Create .gitignore to ensure file is processed
        fs::write(dest_dir.join(".gitignore"), "activate.sh").unwrap();

        // Run path rewriter
        let rewriter = PathRewriter::new(&src_dir, &dest_dir);
        rewriter.rewrite_paths().unwrap();

        // Check that paths were rewritten
        let rewritten_content = fs::read_to_string(dest_dir.join("activate.sh")).unwrap();
        assert!(rewritten_content.contains(&dest_dir.to_string_lossy().to_string()));
        assert!(!rewritten_content.contains(&src_dir.to_string_lossy().to_string()));
    }

    #[test]
    fn test_replace_at_path_boundary_skips_prefix_collision() {
        let src = "/users/me/repo";
        let dest = "/users/me/repo-feature";

        let cases = [
            ("/users/me/repo/foo", "/users/me/repo-feature/foo"),
            ("/users/me/repo", "/users/me/repo-feature"),
            ("a /users/me/repo:b", "a /users/me/repo-feature:b"),
            // prefix collisions left intact
            ("/users/me/repo-archive/foo", "/users/me/repo-archive/foo"),
            ("/users/me/repo_v2", "/users/me/repo_v2"),
            ("/users/me/repo.bak", "/users/me/repo.bak"),
            ("/users/me/repo2", "/users/me/repo2"),
        ];

        for (input, expected) in cases {
            assert_eq!(
                replace_at_path_boundary(input, src, dest),
                expected,
                "input={input}"
            );
        }
    }

    #[test]
    fn test_replace_at_path_boundary_handles_repeated_matches() {
        let out = replace_at_path_boundary("/a/b /a/b/c", "/a/b", "/x");
        assert_eq!(out, "/x /x/c");
    }

    #[test]
    fn test_rewrites_gitignored_file_inside_real_git_repo() {
        // Mirrors the production call site: `dest_path` is a real worktree
        // (i.e. has a `.git` directory) and the file we care about is listed
        // in `.gitignore` — exactly the case the rewriter is documented to
        // handle. Before #161, the `git_ignore(true)` walker filter skipped
        // this file inside a real repo, so the rewrite was a silent no-op.
        let temp_dir = tempdir().unwrap();
        let src_dir = temp_dir.path().join("src");
        let dest_dir = temp_dir.path().join("dest");

        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dest_dir).unwrap();

        let init_ok = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&dest_dir)
            .status()
            .unwrap()
            .success();
        assert!(init_ok, "git init failed");

        let head_path = dest_dir.join(".git/HEAD");
        let head_before = fs::read(&head_path).unwrap();

        fs::write(dest_dir.join(".gitignore"), "activate.sh\n").unwrap();
        let test_content = format!("export PATH=\"{}:$PATH\"", src_dir.display());
        fs::write(dest_dir.join("activate.sh"), &test_content).unwrap();

        PathRewriter::new(&src_dir, &dest_dir)
            .rewrite_paths()
            .unwrap();

        let rewritten = fs::read_to_string(dest_dir.join("activate.sh")).unwrap();
        assert!(
            rewritten.contains(&dest_dir.to_string_lossy().to_string()),
            "expected dest path in rewritten activate.sh: {rewritten}"
        );
        assert!(
            !rewritten.contains(&src_dir.to_string_lossy().to_string()),
            "source path still present in rewritten activate.sh: {rewritten}"
        );

        let head_after = fs::read(&head_path).unwrap();
        assert_eq!(
            head_before, head_after,
            ".git/HEAD must not be touched by the rewriter"
        );
    }

    #[test]
    fn test_rewrite_paths_under_scopes_to_roots_only() {
        let temp_dir = tempdir().unwrap();
        let src_dir = temp_dir.path().join("primary");
        let dest_dir = temp_dir.path().join("worktree");
        fs::create_dir_all(dest_dir.join("node_modules")).unwrap();
        fs::create_dir_all(&src_dir).unwrap();

        // An overlaid file (inside the root we pass) and a tracked file that is
        // NOT in the overlay roots. Both bake in the old primary path.
        let baked = format!("root={}", src_dir.display());
        fs::write(dest_dir.join("node_modules/.bin_path"), &baked).unwrap();
        fs::write(dest_dir.join("tracked.config"), &baked).unwrap();

        PathRewriter::new(&src_dir, &dest_dir)
            .rewrite_paths_under(&[dest_dir.join("node_modules")])
            .unwrap();

        // The overlaid file is rewritten to the new worktree path...
        let overlaid = fs::read_to_string(dest_dir.join("node_modules/.bin_path")).unwrap();
        assert!(overlaid.contains(&dest_dir.to_string_lossy().to_string()));
        assert!(!overlaid.contains(&src_dir.to_string_lossy().to_string()));

        // ...while the tracked file outside the roots is left byte-for-byte
        // identical, so git sees no spurious diff.
        assert_eq!(
            fs::read_to_string(dest_dir.join("tracked.config")).unwrap(),
            baked
        );
    }

    #[test]
    fn test_binary_detection() {
        let rewriter = PathRewriter::new("/tmp", "/tmp2");

        // Text content
        assert!(!rewriter.is_likely_binary("Hello, world!\nThis is text."));

        // Binary-like content with null bytes
        assert!(rewriter.is_likely_binary("Hello\0world"));

        // Content with many non-printable characters
        let binary_like: String = (0..100).map(|_| '\x01').collect();
        assert!(rewriter.is_likely_binary(&binary_like));
    }
}
