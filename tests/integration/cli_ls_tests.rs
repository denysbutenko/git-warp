use crate::integration::cli_surface_support::{
    create_detached_worktree, create_worktree, output_contains_path, setup_test_repo, warp_command,
};
use std::fs;

#[test]
fn test_ls_shows_primary_current_dirty_and_detached_statuses() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    let feature_path = create_worktree(repo_path, "feature-status");
    let detached_path = create_detached_worktree(repo_path, "detached-status");

    fs::write(feature_path.join("dirty.txt"), "changed\n").unwrap();

    let output = warp_command(&feature_path).args(["ls"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("main [primary"), "{stdout}");
    assert!(stdout.contains("feature-status [current dirty"), "{stdout}");
    assert!(stdout.contains("[detached]"), "{stdout}");
    assert!(output_contains_path(&stdout, repo_path), "{stdout}");
    assert!(output_contains_path(&stdout, &detached_path), "{stdout}");
}

#[test]
fn test_ls_orders_current_then_primary_then_dirty_with_summary() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    let alpha_path = create_worktree(repo_path, "alpha");
    let dirty_path = create_worktree(repo_path, "zulu-dirty");
    create_worktree(repo_path, "mike-clean");

    fs::write(dirty_path.join("touch.txt"), "hi\n").unwrap();

    let output = warp_command(&alpha_path).args(["ls"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("📍 Current: alpha"), "{stdout}");
    assert!(stdout.contains("🏠 Primary: main"), "{stdout}");

    let current_idx = stdout.find("alpha [current").expect(&stdout);
    let primary_idx = stdout.find("main [primary").expect(&stdout);
    let dirty_idx = stdout.find("zulu-dirty [dirty").expect(&stdout);
    let clean_idx = stdout.find("mike-clean ").expect(&stdout);

    assert!(current_idx < primary_idx, "{stdout}");
    assert!(primary_idx < dirty_idx, "{stdout}");
    assert!(dirty_idx < clean_idx, "{stdout}");
    assert!(stdout.contains("👉  alpha"), "{stdout}");
    assert!(stdout.contains("⚠️   zulu-dirty"), "{stdout}");
}

#[test]
fn test_ls_interactive_triggers_switcher_dry_run() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    create_worktree(repo_path, "feature/interactive-ls");

    let output = warp_command(repo_path)
        .args(["ls", "--interactive", "--dry-run"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("Would open interactive worktree switcher"));
    assert!(stdout.contains("main"));
    assert!(stdout.contains("feature/interactive-ls"));
}
