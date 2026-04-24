use git_warp::git::GitRepository;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn setup_test_repo() -> tempfile::TempDir {
    let temp_dir = tempdir().unwrap();
    let repo_path = temp_dir.path();

    // Initialize git repo
    Command::new("git")
        .args(&["init", "--initial-branch", "main"])
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

#[test]
fn test_git_repository_find() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    // Test finding repo from root
    std::env::set_current_dir(repo_path).unwrap();
    let found_repo = GitRepository::find();
    assert!(found_repo.is_ok());

    // Test finding repo from subdirectory
    let sub_dir = repo_path.join("subdir");
    fs::create_dir_all(&sub_dir).unwrap();
    std::env::set_current_dir(&sub_dir).unwrap();
    let found_repo = GitRepository::find();
    assert!(found_repo.is_ok());
}

#[test]
fn test_git_repository_not_found() {
    let temp_dir = tempdir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = GitRepository::find();
    assert!(result.is_err());
}

#[test]
fn test_list_worktrees_single() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    std::env::set_current_dir(repo_path).unwrap();

    let git_repo = GitRepository::find().unwrap();
    let worktrees = git_repo.list_worktrees().unwrap();

    assert_eq!(worktrees.len(), 1); // Only main worktree
    assert_eq!(worktrees[0].branch, "main");
    assert_eq!(
        worktrees[0].path.canonicalize().unwrap(),
        repo_path.canonicalize().unwrap()
    );
    assert!(worktrees[0].is_primary);
    assert!(worktrees[0].is_current);
    assert!(!worktrees[0].is_detached);
}

#[test]
fn test_list_worktrees_marks_main_primary_and_linked_current() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path().canonicalize().unwrap();
    let linked_path = repo_path.join("worktrees").join("feature-current");

    std::env::set_current_dir(&repo_path).unwrap();
    let main_repo = GitRepository::find().unwrap();
    main_repo
        .create_worktree_and_branch("feature-current", &linked_path, None)
        .unwrap();

    std::env::set_current_dir(&linked_path).unwrap();
    let linked_repo = GitRepository::find().unwrap();
    let worktrees = linked_repo.list_worktrees().unwrap();

    let main = worktrees
        .iter()
        .find(|w| w.path.canonicalize().unwrap() == repo_path)
        .unwrap();
    let linked = worktrees
        .iter()
        .find(|w| w.path.canonicalize().unwrap() == linked_path)
        .unwrap();

    assert!(main.is_primary);
    assert!(!main.is_current);
    assert!(!linked.is_primary);
    assert!(linked.is_current);
}

#[test]
fn test_cleanup_analysis_excludes_protected_branches() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    std::env::set_current_dir(repo_path).unwrap();

    let git_repo = GitRepository::find().unwrap();

    Command::new("git")
        .args(&["branch", "develop"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let develop_path = repo_path.join("worktrees").join("develop");
    git_repo
        .create_worktree_and_branch("develop", &develop_path, Some("develop"))
        .unwrap();

    let worktrees = git_repo.list_worktrees().unwrap();
    let branch_statuses = git_repo.analyze_branches_for_cleanup(&worktrees).unwrap();

    assert!(branch_statuses.iter().all(|status| status.branch != "main"));
    assert!(
        branch_statuses
            .iter()
            .all(|status| status.branch != "develop")
    );
}

#[test]
fn test_cleanup_analysis_excludes_custom_protected_branches() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    std::env::set_current_dir(repo_path).unwrap();

    let git_repo = GitRepository::find().unwrap();

    Command::new("git")
        .args(&["branch", "staging"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let staging_path = repo_path.join("worktrees").join("staging");
    git_repo
        .create_worktree_and_branch("staging", &staging_path, Some("staging"))
        .unwrap();

    let feature_path = repo_path.join("worktrees").join("feature");
    git_repo
        .create_worktree_and_branch("feature", &feature_path, None)
        .unwrap();

    let worktrees = git_repo.list_worktrees().unwrap();
    let protected_branches = vec!["staging".to_string()];
    let branch_statuses = git_repo
        .analyze_branches_for_cleanup_with_protected_branches(&worktrees, &protected_branches)
        .unwrap();

    assert!(
        branch_statuses
            .iter()
            .all(|status| status.branch != "staging")
    );
    assert!(
        branch_statuses
            .iter()
            .any(|status| status.branch == "feature")
    );
}

#[test]
fn test_create_worktree_and_branch() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    let worktree_path = repo_path.join("worktrees").join("feature-branch");

    std::env::set_current_dir(repo_path).unwrap();
    let git_repo = GitRepository::find().unwrap();

    let result = git_repo.create_worktree_and_branch("feature-branch", &worktree_path, None);
    assert!(result.is_ok());

    // Verify worktree was created
    assert!(worktree_path.exists());
    assert!(worktree_path.join(".git").exists());

    // Verify worktree appears in list
    let worktrees = git_repo.list_worktrees().unwrap();
    assert_eq!(worktrees.len(), 2);

    let feature_worktree = worktrees.iter().find(|w| w.branch == "feature-branch");
    assert!(feature_worktree.is_some());
    assert_eq!(
        feature_worktree.unwrap().path.canonicalize().unwrap(),
        worktree_path.canonicalize().unwrap()
    );
}

#[test]
fn test_remove_worktree() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    let worktree_path = repo_path.join("worktrees").join("temp-branch");

    std::env::set_current_dir(repo_path).unwrap();
    let git_repo = GitRepository::find().unwrap();

    // Create worktree
    git_repo
        .create_worktree_and_branch("temp-branch", &worktree_path, None)
        .unwrap();

    // Verify it exists
    assert!(worktree_path.exists());

    // Remove it
    let result = git_repo.remove_worktree(&worktree_path);
    assert!(result.is_ok());

    // Verify it's gone
    assert!(!worktree_path.exists());

    // Verify it's not in the list
    let worktrees = git_repo.list_worktrees().unwrap();
    let temp_worktree = worktrees.iter().find(|w| w.branch == "temp-branch");
    assert!(temp_worktree.is_none());
}

#[test]
fn test_fetch_branches() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    std::env::set_current_dir(repo_path).unwrap();

    let git_repo = GitRepository::find().unwrap();

    // This should succeed even without remotes (git fetch just does nothing)
    let result = git_repo.fetch_branches();
    assert!(result.is_ok());
}

#[test]
fn test_analyze_branches_for_cleanup() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    std::env::set_current_dir(repo_path).unwrap();

    let git_repo = GitRepository::find().unwrap();

    // Create some branches
    Command::new("git")
        .args(&["checkout", "-b", "feature-1"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(&["checkout", "-b", "feature-2"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Go back to main
    Command::new("git")
        .args(&["checkout", "main"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Create worktrees for the branches
    let feature1_path = repo_path.join("worktrees").join("feature-1");
    let feature2_path = repo_path.join("worktrees").join("feature-2");

    git_repo
        .create_worktree_and_branch("feature-1", &feature1_path, Some("feature-1"))
        .unwrap();
    git_repo
        .create_worktree_and_branch("feature-2", &feature2_path, Some("feature-2"))
        .unwrap();

    let worktrees = git_repo.list_worktrees().unwrap();
    let non_main_worktrees: Vec<_> = worktrees
        .into_iter()
        .filter(|w| w.branch != "main")
        .collect();

    let result = git_repo.analyze_branches_for_cleanup(&non_main_worktrees);
    assert!(result.is_ok());

    let branch_statuses = result.unwrap();
    assert_eq!(branch_statuses.len(), 2);

    // Check that branches are properly analyzed
    for status in &branch_statuses {
        assert!(status.branch == "feature-1" || status.branch == "feature-2");
        assert!(!status.has_uncommitted_changes); // Should be clean
        assert!(!status.has_remote); // No remotes configured
    }
}

#[test]
fn test_delete_branch() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    std::env::set_current_dir(repo_path).unwrap();

    let git_repo = GitRepository::find().unwrap();

    // Create a branch
    Command::new("git")
        .args(&["checkout", "-b", "test-branch"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(&["checkout", "main"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Delete the branch
    let result = git_repo.delete_branch("test-branch", true);
    assert!(result.is_ok());

    // Verify branch is gone
    let output = Command::new("git")
        .args(&["branch", "--list", "test-branch"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    assert!(output.stdout.is_empty());
}

#[test]
fn test_uncommitted_changes_detection() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    std::env::set_current_dir(repo_path).unwrap();

    let git_repo = GitRepository::find().unwrap();

    // Initially should be clean
    let has_changes = git_repo.has_uncommitted_changes(repo_path).unwrap();
    assert!(!has_changes);

    // Make some changes
    fs::write(repo_path.join("new_file.txt"), "New content").unwrap();

    let has_changes = git_repo.has_uncommitted_changes(repo_path).unwrap();
    assert!(has_changes);

    // Stage the changes
    Command::new("git")
        .args(&["add", "."])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let has_changes = git_repo.has_uncommitted_changes(repo_path).unwrap();
    assert!(has_changes); // Staged changes still count

    // Commit the changes
    Command::new("git")
        .args(&["commit", "-m", "Add new file"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let has_changes = git_repo.has_uncommitted_changes(repo_path).unwrap();
    assert!(!has_changes);
}

#[test]
fn test_branch_merge_detection() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    std::env::set_current_dir(repo_path).unwrap();

    // Create a feature branch with some changes
    Command::new("git")
        .args(&["checkout", "-b", "feature"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    fs::write(repo_path.join("feature.txt"), "Feature content").unwrap();

    Command::new("git")
        .args(&["add", "."])
        .current_dir(repo_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(&["commit", "-m", "Add feature"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Go back to main and merge
    Command::new("git")
        .args(&["checkout", "main"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(&["merge", "feature"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let git_repo = GitRepository::find().unwrap();

    // Test if branch is merged
    let is_merged = git_repo.is_branch_merged("feature", "main").unwrap();
    assert!(is_merged);

    // Test with non-merged branch
    Command::new("git")
        .args(&["checkout", "-b", "unmerged"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    fs::write(repo_path.join("unmerged.txt"), "Unmerged content").unwrap();

    Command::new("git")
        .args(&["add", "."])
        .current_dir(repo_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(&["commit", "-m", "Add unmerged feature"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(&["checkout", "main"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let is_merged = git_repo.is_branch_merged("unmerged", "main").unwrap();
    assert!(!is_merged);
}

#[test]
fn test_worktree_path_generation() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    std::env::set_current_dir(repo_path).unwrap();

    let git_repo = GitRepository::find().unwrap();

    let worktree_path = git_repo.get_worktree_path("feature/awesome-feature");

    // Should sanitize branch name and create path
    assert!(
        worktree_path
            .to_string_lossy()
            .contains("feature-awesome-feature")
    );
    assert!(worktree_path.parent().unwrap().ends_with("worktrees"));
}

#[test]
fn test_worktree_path_generation_with_relative_base() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    std::env::set_current_dir(repo_path).unwrap();

    let git_repo = GitRepository::find().unwrap();
    let worktree_path = git_repo.get_worktree_path_with_base(
        "feature/with-custom-base",
        Some(std::path::Path::new(".worktrees")),
    );

    assert!(worktree_path.ends_with(".worktrees/feature-with-custom-base"));
}
