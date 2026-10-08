//! Shared helpers for the split cli_surface integration test modules.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub fn iso_now_minus_hours_millis(hours: i64) -> String {
    use chrono::{Duration, Local, SecondsFormat, Utc};
    (Local::now() - Duration::hours(hours))
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn iso_now_minus_hours_secs(hours: i64) -> String {
    use chrono::{Duration, Local, SecondsFormat, Utc};
    (Local::now() - Duration::hours(hours))
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Secs, false)
}

pub fn run_git(repo_path: &Path, args: &[&str]) {
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

pub fn setup_test_repo() -> tempfile::TempDir {
    setup_test_repo_with_initial_branch("main")
}

pub fn setup_test_repo_with_initial_branch(initial_branch: &str) -> tempfile::TempDir {
    let temp_dir = tempdir().unwrap();
    let repo_path = temp_dir.path();

    run_git(repo_path, &["init", "-b", initial_branch]);
    run_git(repo_path, &["config", "user.email", "test@example.com"]);
    run_git(repo_path, &["config", "user.name", "Test User"]);

    fs::write(repo_path.join("README.md"), "# Test Repository\n").unwrap();
    run_git(repo_path, &["add", "."]);
    run_git(repo_path, &["commit", "-m", "Initial commit"]);

    temp_dir
}

pub fn create_worktree(repo_path: &Path, branch: &str) -> PathBuf {
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

pub fn create_detached_worktree(repo_path: &Path, name: &str) -> PathBuf {
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

pub fn write_codex_session(
    home: &Path,
    cwd: &Path,
    session_id: &str,
    branch: &str,
    timestamp: &str,
) {
    let sessions_dir = home.join(".codex").join("sessions");
    fs::create_dir_all(&sessions_dir).unwrap();
    let cwd = cwd.display().to_string().replace('\\', "\\\\");
    fs::write(
        sessions_dir.join(format!("{session_id}.jsonl")),
        format!(
            r#"{{"timestamp":"{timestamp}","type":"session_meta","payload":{{"id":"{session_id}","timestamp":"{timestamp}","cwd":"{}","originator":"codex-tui","agent_nickname":"Parfit","agent_role":"worker","git":{{"branch":"{branch}"}}}}}}"#,
            cwd
        ),
    )
    .unwrap();
}

pub fn write_live_status(worktree_path: &Path, status: &str, timestamp: &str) {
    let status_path = worktree_path.join(".codex").join("git-warp").join("status");
    fs::create_dir_all(status_path.parent().unwrap()).unwrap();
    fs::write(
        status_path,
        format!(r#"{{"status":"{status}","last_activity":"{timestamp}"}}"#),
    )
    .unwrap();
}

pub fn warp_command(repo_path: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_warp"));
    command.current_dir(repo_path);
    command
}

pub fn normalized_path_text(value: impl AsRef<str>) -> String {
    value
        .as_ref()
        .replace('\\', "/")
        .trim_start_matches("//?/")
        .to_string()
}

pub fn output_contains_path(output: &str, path: &Path) -> bool {
    let output = normalized_path_text(output);
    let display = normalized_path_text(path.display().to_string());
    let canonical = path
        .canonicalize()
        .ok()
        .map(|path| normalized_path_text(path.display().to_string()));

    output.contains(&display) || canonical.is_some_and(|path| output.contains(&path))
}

pub fn write_fake_editor(path: &Path, marker_path: &Path) -> PathBuf {
    #[cfg(windows)]
    let editor_path = path.with_extension("cmd");
    #[cfg(not(windows))]
    let editor_path = path.to_path_buf();

    fs::write(
        &editor_path,
        #[cfg(windows)]
        format!(
            "@echo off\r\necho %~1>\"{}\"\r\nexit /b 0\r\n",
            marker_path.display()
        ),
        #[cfg(not(windows))]
        format!(
            "#!/bin/sh\nprintf '%s' \"$1\" > '{}'\n",
            marker_path.display()
        ),
    )
    .unwrap();

    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&editor_path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&editor_path, permissions).unwrap();
    }

    editor_path
}

pub fn expected_config_path(home: &Path) -> PathBuf {
    home.join(".config").join("git-warp").join("config.toml")
}

#[cfg(unix)]
pub fn write_executable(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(unix)]
pub fn install_script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("install.sh")
}

#[cfg(unix)]
pub fn installer_command(path_dir: &Path, home_dir: &Path) -> Command {
    let mut command = Command::new("/bin/sh");
    let original_path = std::env::var("PATH").unwrap_or_default();
    command
        .arg(install_script_path())
        .env("HOME", home_dir)
        .env("PATH", format!("{}:{original_path}", path_dir.display()))
        .env("GIT_WARP_VERSION", "v9.9.9");
    command
}

#[cfg(unix)]
pub fn uninstall_script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("uninstall.sh")
}

pub fn warp_bin_name() -> String {
    format!("warp{}", std::env::consts::EXE_SUFFIX)
}

pub fn write_fake_warp_binary(path: &Path, version_label: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    #[cfg(unix)]
    {
        write_executable(path, &format!("#!/bin/sh\necho \"{version_label}\"\n"));
    }
    #[cfg(windows)]
    {
        let _ = version_label;
        fs::write(path, b"").unwrap();
    }
}

pub fn setup_repo_with_origin() -> (tempfile::TempDir, tempfile::TempDir) {
    let upstream = tempdir().unwrap();
    Command::new("git")
        .args(["init", "--bare", "-b", "main"])
        .current_dir(upstream.path())
        .output()
        .unwrap();

    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    Command::new("git")
        .args(["remote", "add", "origin"])
        .arg(upstream.path())
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    (temp_dir, upstream)
}
