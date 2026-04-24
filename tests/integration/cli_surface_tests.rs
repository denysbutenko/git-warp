use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

fn run_git(repo_path: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn setup_test_repo() -> tempfile::TempDir {
    let temp_dir = tempdir().unwrap();
    let repo_path = temp_dir.path();

    run_git(repo_path, &["init", "-b", "main"]);
    run_git(repo_path, &["config", "user.email", "test@example.com"]);
    run_git(repo_path, &["config", "user.name", "Test User"]);

    fs::write(repo_path.join("README.md"), "# Test Repository\n").unwrap();
    run_git(repo_path, &["add", "."]);
    run_git(repo_path, &["commit", "-m", "Initial commit"]);

    temp_dir
}

fn create_worktree(repo_path: &Path, branch: &str) -> PathBuf {
    let worktree_path = repo_path.join(".worktrees").join(branch);
    fs::create_dir_all(worktree_path.parent().unwrap()).unwrap();

    let output = Command::new("git")
        .args(["worktree", "add", "-b", branch])
        .arg(&worktree_path)
        .current_dir(repo_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "git worktree add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    worktree_path
}

fn create_detached_worktree(repo_path: &Path, name: &str) -> PathBuf {
    let worktree_path = repo_path.join(".worktrees").join(name);
    fs::create_dir_all(worktree_path.parent().unwrap()).unwrap();

    let output = Command::new("git")
        .args(["worktree", "add", "--detach"])
        .arg(&worktree_path)
        .arg("HEAD")
        .current_dir(repo_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "git worktree add --detach failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    worktree_path
}

fn write_codex_session(home: &Path, cwd: &Path, session_id: &str, branch: &str, timestamp: &str) {
    let sessions_dir = home.join(".codex").join("sessions");
    fs::create_dir_all(&sessions_dir).unwrap();
    fs::write(
        sessions_dir.join(format!("{session_id}.jsonl")),
        format!(
            r#"{{"timestamp":"{timestamp}","type":"session_meta","payload":{{"id":"{session_id}","timestamp":"{timestamp}","cwd":"{}","originator":"codex-tui","agent_nickname":"Parfit","agent_role":"worker","git":{{"branch":"{branch}"}}}}}}"#,
            cwd.display()
        ),
    )
    .unwrap();
}

fn write_live_status(worktree_path: &Path, status: &str, timestamp: &str) {
    let status_path = worktree_path.join(".codex").join("git-warp").join("status");
    fs::create_dir_all(status_path.parent().unwrap()).unwrap();
    fs::write(
        status_path,
        format!(r#"{{"status":"{status}","last_activity":"{timestamp}"}}"#),
    )
    .unwrap();
}

fn warp_command(repo_path: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_warp"));
    command.current_dir(repo_path);
    command
}

fn expected_config_path(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("git-warp")
            .join("config.toml")
    } else if cfg!(target_os = "windows") {
        home.join("AppData")
            .join("Roaming")
            .join("git-warp")
            .join("config.toml")
    } else {
        home.join(".config").join("git-warp").join("config.toml")
    }
}

#[test]
fn test_root_help_hides_removed_global_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .arg("--help")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(!stdout.contains("--always-new"));
    assert!(stdout.contains("shell-config"));
}

#[test]
fn test_switch_help_hides_removed_flags_and_allows_selector_without_branch() {
    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["switch", "--help"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("Usage: warp switch [OPTIONS] [BRANCH]"));
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
        "2026-04-24T10:00:00.000Z",
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
        "2026-04-24T09:00:00.000Z",
    );
    write_live_status(&waiting_worktree, "waiting", "2026-04-24T10:30:00+00:00");
    write_codex_session(
        home_dir.path(),
        &recent_worktree,
        "session-recent",
        "agent-recent",
        "2026-04-24T11:00:00.000Z",
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
    assert!(
        stdout.contains(&repo_path.canonicalize().unwrap().display().to_string()),
        "{stdout}"
    );
    assert!(
        stdout.contains(&detached_path.canonicalize().unwrap().display().to_string()),
        "{stdout}"
    );
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
fn test_config_edit_creates_config_and_launches_editor() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    let home_dir = tempdir().unwrap();
    let config_path = expected_config_path(home_dir.path());
    let marker_path = home_dir.path().join("editor-marker.txt");
    let editor_path = home_dir.path().join("fake-editor.sh");

    fs::write(
        &editor_path,
        format!(
            "#!/bin/sh\nprintf '%s' \"$1\" > '{}'\n",
            marker_path.display()
        ),
    )
    .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&editor_path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&editor_path, permissions).unwrap();
    }

    let output = warp_command(repo_path)
        .env("HOME", home_dir.path())
        .env("EDITOR", &editor_path)
        .env_remove("VISUAL")
        .args(["config", "--edit"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(config_path.exists());
    assert_eq!(
        fs::read_to_string(&marker_path).unwrap(),
        config_path.display().to_string()
    );
}

#[test]
fn test_shell_config_bash_outputs_reusable_function() {
    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["shell-config", "bash"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("warp_cd()"));
    assert!(stdout.contains("warp --terminal echo"));
}
