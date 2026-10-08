use crate::integration::cli_surface_support::{
    create_worktree, iso_now_minus_hours_millis, iso_now_minus_hours_secs, output_contains_path,
    setup_repo_with_origin, setup_test_repo, warp_command, write_codex_session, write_live_status,
};
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_switch_help_hides_removed_flags_and_allows_selector_without_branch() {
    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["switch", "--help"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("Usage:"), "{stdout}");
    assert!(stdout.contains("switch [OPTIONS] [BRANCH]"), "{stdout}");
    assert!(stdout.contains("--latest"));
    assert!(stdout.contains("--waiting"));
    assert!(!stdout.contains("--init"));
    assert!(!stdout.contains("--always-new"));
}

#[test]
fn test_switch_rejects_multiple_target_selectors() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    let output = warp_command(repo_path)
        .args(["switch", "feature/demo", "--latest"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("exactly one of [BRANCH], --latest, or --waiting"));
}

#[test]
fn test_switch_outside_repo_prints_recovery_guidance() {
    let temp_dir = tempdir().unwrap();

    let output = warp_command(temp_dir.path())
        .args(["switch", "feature/demo"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("Not in a Git repository"), "{stderr}");
    assert!(
        stderr.contains("Run this command inside a Git repository"),
        "{stderr}"
    );
}

#[test]
fn test_switch_latest_resolves_branch_from_recent_agent_session() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    let home_dir = tempdir().unwrap();
    let worktree_path = create_worktree(repo_path, "agent-latest");

    write_codex_session(
        home_dir.path(),
        &worktree_path,
        "session-latest",
        "agent-latest",
        &iso_now_minus_hours_millis(1),
    );

    let output = warp_command(repo_path)
        .env("HOME", home_dir.path())
        .args(["--dry-run", "switch", "--latest"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("Would switch to branch 'agent-latest'"));
}

#[test]
fn test_switch_waiting_resolves_branch_from_waiting_agent_session() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    let home_dir = tempdir().unwrap();
    let waiting_worktree = create_worktree(repo_path, "agent-waiting");
    let recent_worktree = create_worktree(repo_path, "agent-recent");

    write_codex_session(
        home_dir.path(),
        &waiting_worktree,
        "session-waiting",
        "agent-waiting",
        &iso_now_minus_hours_millis(3),
    );
    write_live_status(&waiting_worktree, "waiting", &iso_now_minus_hours_secs(2));
    write_codex_session(
        home_dir.path(),
        &recent_worktree,
        "session-recent",
        "agent-recent",
        &iso_now_minus_hours_millis(1),
    );

    let output = warp_command(repo_path)
        .env("HOME", home_dir.path())
        .args(["--dry-run", "switch", "--waiting"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("Would switch to branch 'agent-waiting'"));
}

#[test]
fn test_bare_warp_dry_run_previews_interactive_switcher() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    create_worktree(repo_path, "feature/default-picker");

    let output = warp_command(repo_path).arg("--dry-run").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("Would open interactive worktree switcher"));
    assert!(stdout.contains("main"));
    assert!(stdout.contains("feature/default-picker"));
}

#[test]
fn test_bare_warp_dry_run_marks_only_nested_worktree_current() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    let worktree_path = create_worktree(repo_path, "feature/default-picker");

    let output = warp_command(&worktree_path)
        .arg("--dry-run")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(!stdout.contains("main [current"));
    assert!(stdout.contains("feature/default-picker [current"));
}

#[test]
fn test_switch_dry_run_honors_use_cow_false_config() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    let output = warp_command(repo_path)
        .args(["--dry-run", "switch", "fresh-feature"])
        .env("GIT_WARP_USE_COW", "false")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("Would use traditional Git worktree creation"),
        "dry-run should honor GIT_WARP_USE_COW=false but stdout was: {stdout}"
    );
    assert!(
        !stdout.contains("Would use Copy-on-Write"),
        "dry-run should not announce CoW when disabled: {stdout}"
    );
}

#[test]
fn test_switch_dry_run_labels_new_branch_source() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    let output = warp_command(repo_path)
        .args(["--dry-run", "switch", "fresh-feature"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("Source: new branch 'fresh-feature' from HEAD"),
        "{stdout}"
    );
}

#[test]
fn test_switch_dry_run_labels_local_branch_source() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["branch", "feature-local"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let output = warp_command(repo_path)
        .args(["--dry-run", "switch", "feature-local"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("Source: local branch 'feature-local'"),
        "{stdout}"
    );
}

#[test]
fn test_switch_dry_run_labels_existing_worktree_source() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    let worktree_path = create_worktree(repo_path, "feature-existing");

    let output = warp_command(repo_path)
        .args(["--dry-run", "switch", "feature-existing"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("Source: existing worktree at")
            && output_contains_path(&stdout, &worktree_path),
        "{stdout}"
    );
}

#[test]
fn test_switch_dry_run_labels_remote_branch_source() {
    let (temp_dir, _upstream) = setup_repo_with_origin();
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["branch", "feature-remote"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["push", "origin", "feature-remote"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["branch", "-D", "feature-remote"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let output = warp_command(repo_path)
        .args(["--dry-run", "switch", "feature-remote"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("Source: remote branch 'origin/feature-remote'"),
        "{stdout}"
    );
}

#[test]
fn test_bare_warp_dry_run_marks_local_only_branch_in_switcher() {
    let (temp_dir, _upstream) = setup_repo_with_origin();
    let repo_path = temp_dir.path();
    create_worktree(repo_path, "tracked-feature");
    Command::new("git")
        .args(["push", "origin", "tracked-feature"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    create_worktree(repo_path, "local-feature");

    let output = warp_command(repo_path).arg("--dry-run").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    let local_line = stdout
        .lines()
        .find(|line| line.contains("local-feature"))
        .expect(&stdout);
    assert!(local_line.contains("local-only"), "{local_line}");
    let tracked_line = stdout
        .lines()
        .find(|line| line.contains("tracked-feature"))
        .expect(&stdout);
    assert!(!tracked_line.contains("local-only"), "{tracked_line}");
}
