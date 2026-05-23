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

    /// Rewrite absolute paths in gitignored files
    pub fn rewrite_paths(&self) -> Result<()> {
        let src_str = self.src_path.to_string_lossy();
        let dest_str = self.dest_path.to_string_lossy();

        // Build a list of files to process
        let files: Vec<PathBuf> = WalkBuilder::new(&self.dest_path)
            .hidden(false) // Process hidden files
            .git_ignore(true) // Respect gitignore
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
            .collect();

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
