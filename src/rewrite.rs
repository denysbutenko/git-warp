use crate::error::Result;
use ignore::{DirEntry, WalkBuilder};
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

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

        // Replace paths
        let new_content = content.replace(src_str, dest_str);

        // Write back if content changed
        if new_content != content {
            fs::write(file_path, new_content)?;
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
            let total = content.len();
            if total == 0 {
                return false;
            }

            let printable = content.chars()
                .filter(|c| c.is_ascii_graphic() || c.is_ascii_whitespace())
                .count();

            let printable_ratio = printable as f64 / total as f64;
            printable_ratio < 0.95
        }
    }
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
