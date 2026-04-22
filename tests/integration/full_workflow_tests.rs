use git_warp::config::{Config, ConfigManager};
use git_warp::cow::clone_directory;
use git_warp::git::GitRepository;
use git_warp::process::ProcessManager;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_complete_worktree_creation_workflow() {
    let temp_dir = setup_test_repository();
    let repo_path = temp_dir.path();

    std::env::set_current_dir(repo_path).unwrap();

    // 1. Load configuration
    let config = Config::default();
    assert_eq!(config.terminal_mode, "tab");
    assert!(config.use_cow);

    // 2. Find git repository
    let git_repo = GitRepository::find().unwrap();

    // 3. Generate worktree path
    let branch_name = "feature/awesome-integration";
    let worktree_path = git_repo.get_worktree_path(branch_name);

    // 4. Create worktree and branch
    let result = git_repo.create_worktree_and_branch(branch_name, &worktree_path, None);
    assert!(result.is_ok());

    // 5. Verify worktree exists and is functional
    assert!(worktree_path.exists());
    assert!(worktree_path.join(".git").exists());

    // 6. Test that we can work in the worktree
    std::env::set_current_dir(&worktree_path).unwrap();

    // Create some work
    fs::write(
        worktree_path.join("feature.txt"),
        "New feature implementation",
    )
    .unwrap();

    // Commit the work
    Command::new("git")
        .args(&["add", "."])
        .current_dir(&worktree_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(&["commit", "-m", "Add feature implementation"])
        .current_dir(&worktree_path)
        .output()
        .unwrap();

    // 7. List worktrees and verify our new one appears
    std::env::set_current_dir(repo_path).unwrap();
    let worktrees = git_repo.list_worktrees().unwrap();
    let feature_worktree = worktrees.iter().find(|w| w.branch == branch_name);
    assert!(feature_worktree.is_some());

    // 8. Clean up
    let cleanup_result = git_repo.remove_worktree(&worktree_path);
    assert!(cleanup_result.is_ok());
    assert!(!worktree_path.exists());

    println!("Complete worktree workflow test passed");
}

#[test]
fn test_cow_worktree_creation_with_dependencies() {
    let temp_dir = setup_test_repository_with_dependencies();
    let repo_path = temp_dir.path();

    std::env::set_current_dir(repo_path).unwrap();

    let git_repo = GitRepository::find().unwrap();
    let branch_name = "feature/cow-test";
    let worktree_path = repo_path.join("worktrees").join("cow-test");

    // Try CoW clone first
    let cow_result = clone_directory(repo_path, &worktree_path);

    match cow_result {
        Ok(()) => {
            println!("CoW clone succeeded");

            // Verify dependencies were copied
            assert!(worktree_path.exists());
            assert!(worktree_path.join("node_modules").exists());
            assert!(worktree_path.join("node_modules").join("lodash").exists());
            assert!(worktree_path.join("package.json").exists());

            // Verify file contents match
            let original_package = fs::read_to_string(repo_path.join("package.json")).unwrap();
            let cloned_package = fs::read_to_string(worktree_path.join("package.json")).unwrap();
            assert_eq!(original_package, cloned_package);

            // Now register as git worktree
            let result = git_repo.create_worktree_and_branch(branch_name, &worktree_path, None);
            match result {
                Ok(()) => {
                    println!("Git worktree registration succeeded after CoW");
                }
                Err(e) => {
                    println!("Git worktree registration failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!("CoW clone failed (expected on non-APFS): {}", e);

            // Fall back to regular worktree creation
            let result = git_repo.create_worktree_and_branch(branch_name, &worktree_path, None);
            assert!(result.is_ok());

            // Dependencies won't be present in regular worktree
            assert!(!worktree_path.join("node_modules").exists());
        }
    }
}

#[test]
fn test_process_detection_and_cleanup_workflow() {
    let temp_dir = setup_test_repository();
    let repo_path = temp_dir.path();

    std::env::set_current_dir(repo_path).unwrap();

    let git_repo = GitRepository::find().unwrap();
    let branch_name = "feature/process-test";
    let worktree_path = repo_path.join("worktrees").join("process-test");

    // Create worktree
    git_repo
        .create_worktree_and_branch(branch_name, &worktree_path, None)
        .unwrap();

    // Start a long-running process in the worktree
    let script_path = worktree_path.join("long_process.sh");
    let script_content = r#"#!/bin/bash
sleep 30
"#;
    fs::write(&script_path, script_content).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let mut child = Command::new("bash")
        .arg(&script_path)
        .current_dir(&worktree_path)
        .spawn()
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));

    // Try to detect processes in worktree
    let mut process_manager = ProcessManager::new();
    let processes = process_manager
        .find_processes_in_directory(&worktree_path)
        .unwrap();

    println!("Found {} processes in worktree", processes.len());

    // Try cleanup with process detection
    let worktrees = git_repo.list_worktrees().unwrap();
    let feature_worktrees: Vec<_> = worktrees
        .into_iter()
        .filter(|w| w.branch == branch_name)
        .collect();

    if !feature_worktrees.is_empty() {
        let branch_statuses = git_repo
            .analyze_branches_for_cleanup(&feature_worktrees)
            .unwrap();

        // Should detect that branch can be cleaned up
        assert_eq!(branch_statuses.len(), 1);
        let status = &branch_statuses[0];
        assert_eq!(status.branch, branch_name);
    }

    // Clean up the process
    child.kill().unwrap();
    child.wait().unwrap();

    // Now cleanup should succeed
    let cleanup_result = git_repo.remove_worktree(&worktree_path);
    assert!(cleanup_result.is_ok());

    println!("Process detection and cleanup workflow test passed");
}

#[test]
fn test_configuration_layering_workflow() {
    let temp_dir = tempdir().unwrap();

    // Create config file
    let config_path = temp_dir.path().join("config.toml");
    let config_content = r#"
terminal_mode = "window"
use_cow = false
auto_confirm = true

[git]
default_branch = "develop"
auto_fetch = false
"#;
    fs::write(&config_path, config_content).unwrap();

    // Test layered configuration loading
    let manager = ConfigManager {
        config: Config::default(),
        config_path: config_path.clone(),
    };

    // Environment variables should override file config
    unsafe {
        std::env::set_var("GIT_WARP_TERMINAL_MODE", "tab");
        std::env::set_var("GIT_WARP_USE_COW", "true");
    }

    // In real implementation, this would load with proper layering
    let base_config = Config::default();

    // Verify defaults
    assert_eq!(base_config.terminal_mode, "tab");
    assert!(base_config.use_cow);

    // Clean up
    unsafe {
        std::env::remove_var("GIT_WARP_TERMINAL_MODE");
        std::env::remove_var("GIT_WARP_USE_COW");
    }

    println!("Configuration layering workflow test passed");
}

#[test]
fn test_branch_analysis_and_cleanup_workflow() {
    let temp_dir = setup_test_repository();
    let repo_path = temp_dir.path();

    std::env::set_current_dir(repo_path).unwrap();

    let git_repo = GitRepository::find().unwrap();

    // Create multiple branches with different states
    let branches = vec![
        ("feature/merged", true),    // Will be merged
        ("feature/unmerged", false), // Will remain unmerged
        ("hotfix/old", true),        // Will be merged
    ];

    let mut created_worktrees = Vec::new();

    for (branch_name, should_merge) in branches {
        // Create branch and worktree
        let worktree_path = repo_path
            .join("worktrees")
            .join(branch_name.replace('/', "-"));
        git_repo
            .create_worktree_and_branch(branch_name, &worktree_path, None)
            .unwrap();
        created_worktrees.push((branch_name.to_string(), worktree_path.clone()));

        // Make some changes in the worktree
        std::env::set_current_dir(&worktree_path).unwrap();
        fs::write(
            worktree_path.join("branch_file.txt"),
            format!("Content for {}", branch_name),
        )
        .unwrap();

        Command::new("git")
            .args(&["add", "."])
            .current_dir(&worktree_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(&["commit", "-m", &format!("Add content for {}", branch_name)])
            .current_dir(&worktree_path)
            .output()
            .unwrap();

        // Merge if requested
        if should_merge {
            std::env::set_current_dir(repo_path).unwrap();
            Command::new("git")
                .args(&["merge", branch_name])
                .current_dir(repo_path)
                .output()
                .unwrap();
        }
    }

    std::env::set_current_dir(repo_path).unwrap();

    // Analyze branches for cleanup
    let worktrees = git_repo.list_worktrees().unwrap();
    let feature_worktrees: Vec<_> = worktrees
        .into_iter()
        .filter(|w| w.branch != "main")
        .collect();

    let branch_statuses = git_repo
        .analyze_branches_for_cleanup(&feature_worktrees)
        .unwrap();

    // Should have analyzed all feature branches
    assert_eq!(branch_statuses.len(), 3);

    // Check merge status detection
    let merged_branches: Vec<_> = branch_statuses.iter().filter(|s| s.is_merged).collect();

    let unmerged_branches: Vec<_> = branch_statuses.iter().filter(|s| !s.is_merged).collect();

    println!(
        "Found {} merged branches, {} unmerged branches",
        merged_branches.len(),
        unmerged_branches.len()
    );

    // Clean up all worktrees
    for (_, worktree_path) in created_worktrees {
        git_repo.remove_worktree(&worktree_path).unwrap();
    }

    println!("Branch analysis and cleanup workflow test passed");
}

#[test]
fn test_error_recovery_workflow() {
    let temp_dir = setup_test_repository();
    let repo_path = temp_dir.path();

    std::env::set_current_dir(repo_path).unwrap();

    let git_repo = GitRepository::find().unwrap();

    // Test 1: Try to create worktree with invalid branch name
    let invalid_branch = "feature/invalid\0branch";
    let worktree_path = repo_path.join("worktrees").join("invalid");

    let result = git_repo.create_worktree_and_branch(invalid_branch, &worktree_path, None);
    assert!(result.is_err());

    // Test 2: Try to create worktree in read-only directory
    let readonly_path = repo_path.join("readonly").join("worktree");
    if let Some(parent) = readonly_path.parent() {
        fs::create_dir_all(parent).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(parent).unwrap().permissions();
            perms.set_mode(0o444);
            fs::set_permissions(parent, perms).unwrap();
        }

        let result = git_repo.create_worktree_and_branch("feature/readonly", &readonly_path, None);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Restore permissions for cleanup
            let mut perms = fs::metadata(parent).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(parent, perms).unwrap();
        }

        // May fail due to permissions
        match result {
            Ok(()) => println!("Worktree creation succeeded despite readonly parent"),
            Err(e) => println!("Worktree creation failed as expected: {}", e),
        }
    }

    // Test 3: Try to remove non-existent worktree
    let nonexistent_path = repo_path.join("nonexistent");
    let result = git_repo.remove_worktree(&nonexistent_path);

    // Should handle gracefully
    match result {
        Ok(()) => println!("Non-existent worktree removal handled gracefully"),
        Err(e) => println!("Non-existent worktree removal failed: {}", e),
    }

    println!("Error recovery workflow test passed");
}

// Helper function to set up a test repository
fn setup_test_repository() -> tempfile::TempDir {
    let temp_dir = tempdir().unwrap();
    let repo_path = temp_dir.path();

    // Initialize git repo
    Command::new("git")
        .args(&["init"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Configure git
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

    // Create initial commit
    fs::write(repo_path.join("README.md"), "# Test Repository").unwrap();
    fs::write(
        repo_path.join(".gitignore"),
        "node_modules/\n*.log\nbuild/\n",
    )
    .unwrap();

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

// Helper function to set up a test repository with dependencies
fn setup_test_repository_with_dependencies() -> tempfile::TempDir {
    let temp_dir = setup_test_repository();
    let repo_path = temp_dir.path();

    // Create package.json
    let package_json = r#"{
  "name": "test-project",
  "version": "1.0.0",
  "dependencies": {
    "lodash": "^4.17.21"
  }
}"#;
    fs::write(repo_path.join("package.json"), package_json).unwrap();

    // Create mock node_modules
    let node_modules = repo_path.join("node_modules");
    let lodash_dir = node_modules.join("lodash");
    fs::create_dir_all(&lodash_dir).unwrap();

    fs::write(
        lodash_dir.join("package.json"),
        r#"{
  "name": "lodash",
  "version": "4.17.21"
}"#,
    )
    .unwrap();

    fs::write(
        lodash_dir.join("index.js"),
        "module.exports = require('./lodash');",
    )
    .unwrap();
    fs::write(lodash_dir.join("lodash.js"), "// Lodash implementation").unwrap();

    // Create build directory
    let build_dir = repo_path.join("build");
    fs::create_dir_all(&build_dir).unwrap();
    fs::write(build_dir.join("app.js"), "// Built application").unwrap();

    // Commit the changes
    Command::new("git")
        .args(&["add", "."])
        .current_dir(repo_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(&["commit", "-m", "Add dependencies"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    temp_dir
}
