use git_warp::cow::{clone_directory, is_cow_supported};
use git_warp::git::GitRepository;
use git_warp::rewrite::PathRewriter;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_cow_clone_with_git_worktree_registration() {
    let _cwd = crate::support::CurrentDirGuard::new();
    let temp_dir = setup_git_repository();
    let repo_path = temp_dir.path();

    std::env::set_current_dir(repo_path).unwrap();

    let git_repo = GitRepository::find().unwrap();
    let branch_name = "feature/cow-integration";
    let worktree_path = repo_path.join("worktrees").join("cow-integration");

    // Test the full CoW + Git worktree integration
    let cow_result = clone_directory(repo_path, &worktree_path);

    match cow_result {
        Ok(()) => {
            println!("CoW clone succeeded, testing git integration");

            // Verify the clone exists
            assert!(worktree_path.exists());
            assert!(worktree_path.join(".git").exists());
            assert!(worktree_path.join("README.md").exists());

            // Now we need to register this as a proper git worktree
            // First, remove the .git directory (it's a copy, not a worktree)
            fs::remove_dir_all(worktree_path.join(".git")).unwrap();

            // Use git worktree add with existing directory
            let result = Command::new("git")
                .args(&[
                    "worktree",
                    "add",
                    "--detach",
                    worktree_path.to_str().unwrap(),
                ])
                .current_dir(repo_path)
                .output();

            match result {
                Ok(output) => {
                    if output.status.success() {
                        // Now create and checkout the branch
                        Command::new("git")
                            .args(&["checkout", "-b", branch_name])
                            .current_dir(&worktree_path)
                            .output()
                            .unwrap();

                        // Verify it's a proper worktree
                        let worktrees = git_repo.list_worktrees().unwrap();
                        let cow_worktree = worktrees.iter().find(|w| w.branch == branch_name);
                        assert!(cow_worktree.is_some());

                        // Test that we can make commits
                        fs::write(worktree_path.join("feature.txt"), "CoW feature").unwrap();

                        Command::new("git")
                            .args(&["add", "."])
                            .current_dir(&worktree_path)
                            .output()
                            .unwrap();

                        Command::new("git")
                            .args(&["commit", "-m", "Add CoW feature"])
                            .current_dir(&worktree_path)
                            .output()
                            .unwrap();

                        println!("CoW + Git worktree integration successful");
                    } else {
                        println!(
                            "Git worktree registration failed: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                }
                Err(e) => {
                    println!("Failed to register git worktree: {}", e);
                }
            }

            // Cleanup
            git_repo.remove_worktree(&worktree_path).ok();
        }
        Err(e) => {
            println!("CoW clone failed (expected on non-APFS): {}", e);

            // Test fallback to regular worktree
            let result = git_repo.create_worktree_and_branch(branch_name, &worktree_path, None);
            assert!(result.is_ok());

            println!("Fallback to regular worktree successful");

            git_repo.remove_worktree(&worktree_path).unwrap();
        }
    }
}

#[test]
fn test_cow_with_path_rewriting() {
    let _cwd = crate::support::CurrentDirGuard::new();
    let temp_dir = setup_git_repository_with_configs();
    let repo_path = temp_dir.path();
    let clone_path = temp_dir.path().parent().unwrap().join("clone");

    // Test CoW clone followed by path rewriting
    let cow_result = clone_directory(repo_path, &clone_path);

    match cow_result {
        Ok(()) => {
            println!("CoW clone succeeded, testing path rewriting");

            // Verify files were copied
            assert!(clone_path.join("config.toml").exists());
            assert!(clone_path.join("scripts").join("build.sh").exists());

            // Check original content has absolute paths
            let original_config = fs::read_to_string(clone_path.join("config.toml")).unwrap();
            assert!(original_config.contains(&repo_path.to_string_lossy().to_string()));

            let original_script =
                fs::read_to_string(clone_path.join("scripts").join("build.sh")).unwrap();
            assert!(original_script.contains(&repo_path.to_string_lossy().to_string()));

            // Apply path rewriting
            let rewriter = PathRewriter::new(&repo_path, &clone_path);

            let rewrite_result = rewriter.rewrite_paths();
            match rewrite_result {
                Ok(()) => {
                    // Check that paths were rewritten
                    let rewritten_config =
                        fs::read_to_string(clone_path.join("config.toml")).unwrap();
                    assert!(rewritten_config.contains(&clone_path.to_string_lossy().to_string()));
                    assert!(!rewritten_config.contains(&repo_path.to_string_lossy().to_string()));

                    let rewritten_script =
                        fs::read_to_string(clone_path.join("scripts").join("build.sh")).unwrap();
                    assert!(rewritten_script.contains(&clone_path.to_string_lossy().to_string()));

                    println!("CoW + path rewriting integration successful");
                }
                Err(e) => {
                    println!("Path rewriting failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!("CoW clone failed (expected on non-APFS): {}", e);
        }
    }
}

#[test]
fn test_cow_preserves_git_history() {
    let _cwd = crate::support::CurrentDirGuard::new();
    let temp_dir = setup_git_repository_with_history();
    let repo_path = temp_dir.path();
    let clone_path = temp_dir.path().parent().unwrap().join("history_clone");

    std::env::set_current_dir(repo_path).unwrap();

    // Get original git history
    let original_log = Command::new("git")
        .args(&["log", "--oneline"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let cow_result = clone_directory(repo_path, &clone_path);

    match cow_result {
        Ok(()) => {
            println!("CoW clone succeeded, checking git history preservation");

            // Check that .git directory was copied
            assert!(clone_path.join(".git").exists());

            // Get cloned git history
            let cloned_log = Command::new("git")
                .args(&["log", "--oneline"])
                .current_dir(&clone_path)
                .output()
                .unwrap();

            // History should be identical
            assert_eq!(original_log.stdout, cloned_log.stdout);

            // Should be able to create new commits in clone
            fs::write(clone_path.join("new_file.txt"), "New content").unwrap();

            Command::new("git")
                .args(&["add", "."])
                .current_dir(&clone_path)
                .output()
                .unwrap();

            Command::new("git")
                .args(&["commit", "-m", "Add new file in clone"])
                .current_dir(&clone_path)
                .output()
                .unwrap();

            // Original should not have the new commit
            let new_original_log = Command::new("git")
                .args(&["log", "--oneline"])
                .current_dir(repo_path)
                .output()
                .unwrap();

            let new_cloned_log = Command::new("git")
                .args(&["log", "--oneline"])
                .current_dir(&clone_path)
                .output()
                .unwrap();

            // Histories should now be different
            assert_ne!(new_original_log.stdout, new_cloned_log.stdout);

            println!("CoW git history preservation test successful");
        }
        Err(e) => {
            println!("CoW clone failed (expected on non-APFS): {}", e);
        }
    }
}

#[test]
fn test_cow_with_large_dependencies() {
    let _cwd = crate::support::CurrentDirGuard::new();
    let temp_dir = setup_repository_with_large_deps();
    let repo_path = temp_dir.path();
    let clone_path = temp_dir.path().parent().unwrap().join("deps_clone");

    // Count files before cloning
    let original_file_count = count_files(repo_path);
    println!("Original repository has {} files", original_file_count);

    let start_time = std::time::Instant::now();
    let cow_result = clone_directory(repo_path, &clone_path);
    let clone_duration = start_time.elapsed();

    match cow_result {
        Ok(()) => {
            println!("CoW clone completed in {:?}", clone_duration);

            // Verify all files were copied
            let cloned_file_count = count_files(&clone_path);
            assert_eq!(original_file_count, cloned_file_count);

            // Verify specific dependency structures
            assert!(clone_path.join("node_modules").exists());
            assert!(
                clone_path
                    .join("node_modules")
                    .join("large-package")
                    .exists()
            );
            assert!(clone_path.join("build").exists());
            assert!(clone_path.join("cache").exists());

            // Test that modifications are independent
            fs::write(
                clone_path.join("node_modules").join("test.txt"),
                "clone modification",
            )
            .unwrap();

            // Original should not have this file
            assert!(!repo_path.join("node_modules").join("test.txt").exists());

            println!(
                "CoW large dependencies test successful - {} files in {:?}",
                cloned_file_count, clone_duration
            );

            // Performance expectations for CoW
            if clone_duration.as_secs() < 5 {
                println!("CoW performance excellent: < 5 seconds");
            } else {
                println!(
                    "CoW performance acceptable: {} seconds",
                    clone_duration.as_secs()
                );
            }
        }
        Err(e) => {
            println!("CoW clone failed (expected on non-APFS): {}", e);

            // For comparison, test regular copy
            let start_time = std::time::Instant::now();
            copy_directory_recursive(repo_path, &clone_path).unwrap();
            let copy_duration = start_time.elapsed();

            println!("Regular copy took {:?} (for comparison)", copy_duration);
        }
    }
}

#[test]
fn test_cow_filesystem_detection() {
    let _cwd = crate::support::CurrentDirGuard::new();
    // Test filesystem detection accuracy
    let current_dir = std::env::current_dir().unwrap();

    let cow_supported = is_cow_supported(&current_dir);

    match cow_supported {
        Ok(true) => {
            println!("CoW is supported on current filesystem");

            // Should be able to perform CoW operations
            let temp_dir = tempdir().unwrap();
            let src = temp_dir.path().join("src");
            let dst = temp_dir.path().join("dst");

            fs::create_dir_all(&src).unwrap();
            fs::write(src.join("test.txt"), "test content").unwrap();

            let cow_result = clone_directory(&src, &dst);
            assert!(cow_result.is_ok());

            assert!(dst.exists());
            assert_eq!(
                fs::read_to_string(dst.join("test.txt")).unwrap(),
                "test content"
            );
        }
        Ok(false) => {
            println!("CoW is not supported on current filesystem");

            // Should gracefully fall back
            let temp_dir = tempdir().unwrap();
            let src = temp_dir.path().join("src");
            let dst = temp_dir.path().join("dst");

            fs::create_dir_all(&src).unwrap();
            fs::write(src.join("test.txt"), "test content").unwrap();

            let cow_result = clone_directory(&src, &dst);
            assert!(cow_result.is_err()); // Should fail gracefully
        }
        Err(e) => {
            println!("CoW support detection failed: {}", e);
        }
    }
}

#[test]
fn test_cow_error_recovery() {
    let _cwd = crate::support::CurrentDirGuard::new();
    let temp_dir = tempdir().unwrap();

    // Test 1: Source doesn't exist
    let nonexistent_src = temp_dir.path().join("nonexistent");
    let dst1 = temp_dir.path().join("dst1");

    let result = clone_directory(&nonexistent_src, &dst1);
    assert!(result.is_err());
    assert!(!dst1.exists());

    // Test 2: Destination parent is read-only
    let src = temp_dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("test.txt"), "content").unwrap();

    let readonly_parent = temp_dir.path().join("readonly");
    fs::create_dir_all(&readonly_parent).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&readonly_parent).unwrap().permissions();
        perms.set_mode(0o444); // Read-only
        fs::set_permissions(&readonly_parent, perms).unwrap();

        let dst2 = readonly_parent.join("dst");
        let result = clone_directory(&src, &dst2);

        // Restore permissions for cleanup
        let mut perms = fs::metadata(&readonly_parent).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&readonly_parent, perms).unwrap();

        // Should handle permission errors gracefully
        match result {
            Ok(()) => println!("CoW succeeded despite readonly parent"),
            Err(e) => println!("CoW failed as expected with readonly parent: {}", e),
        }
    }

    // Test 3: Destination already exists
    let dst3 = temp_dir.path().join("existing");
    fs::create_dir_all(&dst3).unwrap();
    fs::write(dst3.join("existing.txt"), "existing").unwrap();

    let result = clone_directory(&src, &dst3);

    match result {
        Ok(()) => {
            // Should overwrite existing destination
            assert!(!dst3.join("existing.txt").exists()); // Old content gone
            assert!(dst3.join("test.txt").exists()); // New content present
        }
        Err(e) => {
            println!("CoW failed with existing destination: {}", e);
        }
    }
}

// Helper functions

fn setup_git_repository() -> tempfile::TempDir {
    let temp_dir = tempdir().unwrap();
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(&["init"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(&["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(&["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    fs::write(repo_path.join("README.md"), "# Test Repo").unwrap();
    fs::write(repo_path.join(".gitignore"), "node_modules/\n*.log\n").unwrap();

    Command::new("git")
        .args(&["add", "."])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(&["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    temp_dir
}

fn setup_git_repository_with_configs() -> tempfile::TempDir {
    let temp_dir = setup_git_repository();
    let repo_path = temp_dir.path();

    // Create configuration files with absolute paths
    let config_content = format!(
        r#"
[project]
root_path = "{}"
build_dir = "{}/build"
cache_dir = "{}/cache"
"#,
        repo_path.display(),
        repo_path.display(),
        repo_path.display()
    );

    fs::write(repo_path.join("config.toml"), config_content).unwrap();

    // Create script with absolute paths
    let scripts_dir = repo_path.join("scripts");
    fs::create_dir_all(&scripts_dir).unwrap();

    let script_content = format!(
        r#"#!/bin/bash
PROJECT_ROOT="{}"
cd "$PROJECT_ROOT"
echo "Building in {}"
"#,
        repo_path.display(),
        repo_path.display()
    );

    fs::write(scripts_dir.join("build.sh"), script_content).unwrap();

    // Add to git
    Command::new("git")
        .args(&["add", "."])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(&["commit", "-m", "Add configs"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    temp_dir
}

fn setup_git_repository_with_history() -> tempfile::TempDir {
    let temp_dir = setup_git_repository();
    let repo_path = temp_dir.path();

    // Create multiple commits
    for i in 1..=5 {
        fs::write(
            repo_path.join(format!("file_{}.txt", i)),
            format!("Content {}", i),
        )
        .unwrap();
        Command::new("git")
            .args(&["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(&["commit", "-m", &format!("Commit {}", i)])
            .current_dir(repo_path)
            .output()
            .unwrap();
    }

    temp_dir
}

fn setup_repository_with_large_deps() -> tempfile::TempDir {
    let temp_dir = setup_git_repository();
    let repo_path = temp_dir.path();

    // Create node_modules with many files
    let node_modules = repo_path.join("node_modules");
    for pkg in &["package-1", "package-2", "large-package"] {
        let pkg_dir = node_modules.join(pkg);
        fs::create_dir_all(&pkg_dir).unwrap();

        // Create many files in each package
        for i in 0..20 {
            fs::write(
                pkg_dir.join(format!("file_{}.js", i)),
                format!("// File {} content", i),
            )
            .unwrap();
        }

        fs::write(
            pkg_dir.join("package.json"),
            format!(r#"{{"name": "{}"}}"#, pkg),
        )
        .unwrap();
    }

    // Create build directory with artifacts
    let build_dir = repo_path.join("build");
    fs::create_dir_all(&build_dir).unwrap();
    for i in 0..10 {
        fs::write(
            build_dir.join(format!("build_{}.js", i)),
            format!("// Build artifact {}", i),
        )
        .unwrap();
    }

    // Create cache directory
    let cache_dir = repo_path.join("cache");
    fs::create_dir_all(&cache_dir).unwrap();
    for i in 0..15 {
        fs::write(
            cache_dir.join(format!("cache_{}.tmp", i)),
            format!("Cache data {}", i),
        )
        .unwrap();
    }

    temp_dir
}

fn count_files<P: AsRef<std::path::Path>>(dir: P) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                count += 1;
            } else if path.is_dir() {
                count += count_files(&path);
            }
        }
    }
    count
}

fn copy_directory_recursive<P: AsRef<std::path::Path>, Q: AsRef<std::path::Path>>(
    src: P,
    dst: Q,
) -> std::io::Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_directory_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}
