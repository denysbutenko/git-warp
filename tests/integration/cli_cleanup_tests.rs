use crate::integration::cli_surface_support::{
    create_worktree, setup_test_repo, setup_test_repo_with_initial_branch, warp_command,
};
use std::fs;
use std::process::Command;

#[test]
fn test_cleanup_uses_primary_branch_as_base_and_prints_candidate_reasons() {
    let temp_dir = setup_test_repo_with_initial_branch("trunk");
    let repo_path = temp_dir.path();
    let worktree_path = create_worktree(repo_path, "feature/merged");

    let output = warp_command(repo_path)
        .args(["--auto-confirm", "cleanup", "--mode", "merged", "--no-kill"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("feature/merged at"),
        "candidate branch should be printed: {stdout}"
    );
    assert!(
        stdout.contains("[merged; no remote; clean]"),
        "candidate reasons should be visible: {stdout}"
    );
    assert!(
        stdout.contains("Removed worktree and branch: feature/merged"),
        "cleanup should remove the merged worktree and branch: {stdout}"
    );
    assert!(!worktree_path.exists());
}

#[test]
fn test_cleanup_returns_error_when_removal_fails() {
    let temp_dir = setup_test_repo_with_initial_branch("trunk");
    let repo_path = temp_dir.path();
    let worktree_path = create_worktree(repo_path, "feature/locked");

    let lock_output = Command::new("git")
        .args(["worktree", "lock", "--reason", "held for test"])
        .arg(&worktree_path)
        .current_dir(repo_path)
        .output()
        .unwrap();
    assert!(
        lock_output.status.success(),
        "git worktree lock failed: {}",
        String::from_utf8_lossy(&lock_output.stderr)
    );

    let output = warp_command(repo_path)
        .args(["--auto-confirm", "cleanup", "--mode", "merged", "--no-kill"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "cleanup should exit non-zero when a removal fails; stdout={stdout}, stderr={stderr}"
    );
    assert!(
        stdout.contains("Failed to remove worktree feature/locked"),
        "cleanup should report the failed removal: {stdout}"
    );
    assert!(
        stdout.contains("Cleanup complete: 0 removed, 1 failed"),
        "cleanup should still print the summary: {stdout}"
    );
    assert!(worktree_path.exists());
}

#[test]
fn test_cleanup_rejects_unknown_mode() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    let output = warp_command(repo_path)
        .args(["cleanup", "--mode", "bogus"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit when --mode is unknown; stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    for variant in ["all", "merged", "remoteless", "interactive"] {
        assert!(
            stderr.contains(variant),
            "stderr should mention valid mode `{variant}`: {stderr}",
        );
    }
}

#[test]
fn test_cleanup_rejects_kill_and_no_kill_together() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    let output = warp_command(repo_path)
        .args(["cleanup", "--mode", "merged", "--kill", "--no-kill"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit when --kill and --no-kill are combined; stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--kill") && stderr.contains("--no-kill"),
        "stderr should mention both flags: {stderr}"
    );
}

#[test]
fn test_cleanup_dry_run_explains_candidates_and_skipped_reasons() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    create_worktree(repo_path, "feature/eligible");

    // A protected branch worktree (`develop` is protected by default).
    Command::new("git")
        .args(["branch", "develop"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    let develop_path = repo_path.join(".worktrees").join("develop");
    fs::create_dir_all(develop_path.parent().unwrap()).unwrap();
    Command::new("git")
        .args(["worktree", "add"])
        .arg(&develop_path)
        .arg("develop")
        .current_dir(repo_path)
        .output()
        .unwrap();

    let output = warp_command(repo_path)
        .args(["--dry-run", "cleanup", "--mode", "all"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("Dry run: previewing cleanup with mode: all"),
        "dry-run header missing: {stdout}"
    );
    assert!(
        stdout.contains("Cleanup base branch: main"),
        "base branch should be reported: {stdout}"
    );
    assert!(
        stdout.contains("Skipped (not eligible for cleanup):"),
        "skipped section header missing: {stdout}"
    );
    assert!(
        stdout.contains("[primary worktree]"),
        "primary worktree should be skipped with reason: {stdout}"
    );
    assert!(
        stdout.contains("develop") && stdout.contains("[protected branch]"),
        "develop should be skipped as protected: {stdout}"
    );
    assert!(
        stdout.contains("feature/eligible"),
        "feature/eligible should appear as candidate: {stdout}"
    );
    let candidate_line = stdout
        .lines()
        .find(|line| line.contains("feature/eligible") && line.contains("•"))
        .expect("candidate row missing");
    assert!(
        candidate_line.contains("[") && candidate_line.contains("; clean"),
        "candidate row should include reason and clean/dirty tags: {candidate_line}"
    );
    assert!(
        stdout.contains("Dry run complete: no worktrees were removed."),
        "dry-run footer missing: {stdout}"
    );
}
