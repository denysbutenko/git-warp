use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn iso_now_minus_hours_millis(hours: i64) -> String {
    use chrono::{Duration, Local, SecondsFormat, Utc};
    (Local::now() - Duration::hours(hours))
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn iso_now_minus_hours_secs(hours: i64) -> String {
    use chrono::{Duration, Local, SecondsFormat, Utc};
    (Local::now() - Duration::hours(hours))
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Secs, false)
}

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
    setup_test_repo_with_initial_branch("main")
}

fn setup_test_repo_with_initial_branch(initial_branch: &str) -> tempfile::TempDir {
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

fn normalized_path_text(value: impl AsRef<str>) -> String {
    value
        .as_ref()
        .replace('\\', "/")
        .trim_start_matches("//?/")
        .to_string()
}

fn output_contains_path(output: &str, path: &Path) -> bool {
    let output = normalized_path_text(output);
    let display = normalized_path_text(path.display().to_string());
    let canonical = path
        .canonicalize()
        .ok()
        .map(|path| normalized_path_text(path.display().to_string()));

    output.contains(&display) || canonical.is_some_and(|path| output.contains(&path))
}

fn write_fake_editor(path: &Path, marker_path: &Path) -> PathBuf {
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

fn expected_config_path(home: &Path) -> PathBuf {
    home.join(".config").join("git-warp").join("config.toml")
}

#[cfg(unix)]
fn write_executable(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(unix)]
fn install_script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("install.sh")
}

#[cfg(unix)]
fn installer_command(path_dir: &Path, home_dir: &Path) -> Command {
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
#[test]
fn test_installer_explains_unsupported_platform() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();

    write_executable(
        &fake_bin.path().join("uname"),
        r#"#!/bin/sh
if [ "$1" = "-s" ]; then
  echo Plan9
else
  echo sparc
fi
"#,
    );

    let output = installer_command(fake_bin.path(), home_dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains("unsupported operating system: Plan9"),
        "{stderr}"
    );
    assert!(stderr.contains("Supported prebuilt targets:"), "{stderr}");
    assert!(stderr.contains("GIT_WARP_INSTALL_METHOD=cargo"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn test_installer_explains_unsupported_architecture() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();

    write_executable(
        &fake_bin.path().join("uname"),
        r#"#!/bin/sh
if [ "$1" = "-s" ]; then
  echo Darwin
else
  echo powerpc
fi
"#,
    );

    let output = installer_command(fake_bin.path(), home_dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains("unsupported CPU architecture: powerpc"),
        "{stderr}"
    );
    assert!(stderr.contains("Supported prebuilt targets:"), "{stderr}");
    assert!(stderr.contains("GIT_WARP_INSTALL_METHOD=cargo"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn test_installer_explains_failed_binary_download() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();

    write_executable(
        &fake_bin.path().join("uname"),
        r#"#!/bin/sh
if [ "$1" = "-s" ]; then
  echo Darwin
else
  echo arm64
fi
"#,
    );
    write_executable(
        &fake_bin.path().join("curl"),
        r#"#!/bin/sh
echo "not found" >&2
exit 22
"#,
    );

    let output = installer_command(fake_bin.path(), home_dir.path())
        .env("GIT_WARP_DOWNLOAD_BASE", "https://example.invalid/releases")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains(
            "failed to download https://example.invalid/releases/git-warp-v9.9.9-aarch64-apple-darwin.tar.gz"
        ),
        "{stderr}"
    );
    assert!(
        stderr.contains("The release asset may not exist yet for this platform or version."),
        "{stderr}"
    );
    assert!(stderr.contains("GIT_WARP_INSTALL_METHOD=cargo"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn test_installer_explains_cargo_fallback_failure() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();

    write_executable(
        &fake_bin.path().join("cargo"),
        r#"#!/bin/sh
echo "cargo exploded" >&2
exit 101
"#,
    );

    let output = installer_command(fake_bin.path(), home_dir.path())
        .env("GIT_WARP_INSTALL_METHOD", "cargo")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("cargo exploded"), "{stderr}");
    assert!(
        stderr.contains("Cargo install failed for Git-Warp v9.9.9."),
        "{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn test_installer_prints_actionable_path_guidance() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();

    write_executable(
        &fake_bin.path().join("uname"),
        r#"#!/bin/sh
if [ "$1" = "-s" ]; then
  echo Linux
else
  echo x86_64
fi
"#,
    );
    write_executable(
        &fake_bin.path().join("curl"),
        r#"#!/bin/sh
url=""
output=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) shift; output="$1" ;;
    -O) shift; output="$1" ;;
    http*|https*) url="$1" ;;
  esac
  shift
done
case "$url" in
  *.sha256)
    archive_name="${url##*/}"
    archive_name="${archive_name%.sha256}"
    printf 'b5d54c39e66671c9731b9f471e585d8262cd4f54963f0c93082d8dcf334d4c78  %s\n' "$archive_name" > "$output"
    ;;
  *)
    printf fake > "$output"
    ;;
esac
"#,
    );
    write_executable(
        &fake_bin.path().join("tar"),
        r#"#!/bin/sh
dest=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-C" ]; then
    shift
    dest="$1"
  fi
  shift
done
cat > "$dest/warp" <<'SCRIPT'
#!/bin/sh
echo warp 9.9.9
SCRIPT
chmod 755 "$dest/warp"
"#,
    );

    let output = installer_command(fake_bin.path(), home_dir.path())
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(stdout.contains("warp 9.9.9"), "{stdout}");
    assert!(
        stdout.contains(&format!(
            "Add {} to PATH so your shell can find 'warp':",
            install_dir.path().display()
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "export PATH=\"{}:$PATH\"",
            install_dir.path().display()
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains("Open a new terminal or run 'warp doctor' after updating PATH."),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn test_installer_fails_on_checksum_mismatch() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();

    write_executable(
        &fake_bin.path().join("uname"),
        r#"#!/bin/sh
if [ "$1" = "-s" ]; then
  echo Linux
else
  echo x86_64
fi
"#,
    );
    write_executable(
        &fake_bin.path().join("curl"),
        r#"#!/bin/sh
url=""
output=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) shift; output="$1" ;;
    -O) shift; output="$1" ;;
    http*|https*) url="$1" ;;
  esac
  shift
done
case "$url" in
  *.sha256)
    archive_name="${url##*/}"
    archive_name="${archive_name%.sha256}"
    printf '0000000000000000000000000000000000000000000000000000000000000000  %s\n' "$archive_name" > "$output"
    ;;
  *)
    printf fake > "$output"
    ;;
esac
"#,
    );

    let output = installer_command(fake_bin.path(), home_dir.path())
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains(
            "checksum verification failed for git-warp-v9.9.9-x86_64-unknown-linux-gnu.tar.gz"
        ),
        "{stderr}"
    );
    assert!(
        stderr
            .contains("expected: 0000000000000000000000000000000000000000000000000000000000000000"),
        "{stderr}"
    );
    assert!(
        stderr
            .contains("actual:   b5d54c39e66671c9731b9f471e585d8262cd4f54963f0c93082d8dcf334d4c78"),
        "{stderr}"
    );
    assert!(stderr.contains("GIT_WARP_INSTALL_METHOD=cargo"), "{stderr}");
    assert!(
        !install_dir.path().join("warp").exists(),
        "warp binary should not be installed on checksum mismatch"
    );
}

#[cfg(unix)]
#[test]
fn test_installer_skips_checksum_when_opted_out() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();

    write_executable(
        &fake_bin.path().join("uname"),
        r#"#!/bin/sh
if [ "$1" = "-s" ]; then
  echo Linux
else
  echo x86_64
fi
"#,
    );
    write_executable(
        &fake_bin.path().join("curl"),
        r#"#!/bin/sh
output=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-o" ]; then
    shift
    output="$1"
  fi
  shift
done
printf fake > "$output"
"#,
    );
    write_executable(
        &fake_bin.path().join("tar"),
        r#"#!/bin/sh
dest=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-C" ]; then
    shift
    dest="$1"
  fi
  shift
done
cat > "$dest/warp" <<'SCRIPT'
#!/bin/sh
echo warp 9.9.9
SCRIPT
chmod 755 "$dest/warp"
"#,
    );

    let output = installer_command(fake_bin.path(), home_dir.path())
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .env("GIT_WARP_SKIP_CHECKSUM", "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains("Skipping checksum verification"),
        "{stdout}"
    );
    assert!(!stderr.contains("checksum verification failed"), "{stderr}");
    assert!(install_dir.path().join("warp").exists());
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
fn test_root_help_shows_doctor_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .arg("--help")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("doctor"));
    assert!(stdout.contains("Check Git-Warp setup and print next steps"));
}

#[test]
fn test_root_help_shows_release_check_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .arg("--help")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("release-check"));
    assert!(stdout.contains("Validate release metadata and smoke checks"));
}

#[test]
fn test_release_check_metadata_only_accepts_current_release_metadata() {
    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args([
            "release-check",
            "--metadata-only",
            "--version",
            concat!("v", env!("CARGO_PKG_VERSION")),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains("Release metadata checks passed"),
        "{stdout}"
    );
    assert!(
        stdout.contains(concat!("v", env!("CARGO_PKG_VERSION"))),
        "{stdout}"
    );
}

#[test]
fn test_release_check_metadata_only_rejects_missing_future_release_updates() {
    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["release-check", "--metadata-only", "--version", "v0.4.0"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "{stdout}");
    assert!(
        stderr.contains("Cargo.toml package version is 0.3.0, expected 0.4.0"),
        "{stderr}"
    );
    assert!(
        normalized_path_text(&stderr).contains("docs/releases/v0.4.0.md is missing"),
        "{stderr}"
    );
}

#[test]
fn test_doctor_outside_repo_prints_recovery_guidance() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();

    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("Git-Warp Doctor"));
    assert!(stdout.contains("Config file"));
    assert!(stdout.contains("Git repository"));
    assert!(stdout.contains("Run this command inside a Git repository"));
    assert!(stdout.contains("warp hooks-install --level user --runtime all"));
    assert!(stdout.contains("warp switch --no-cow <branch>"));
}

#[test]
fn test_doctor_inside_repo_prints_repo_and_worktree_checks() {
    let temp_dir = setup_test_repo();
    let home_dir = tempdir().unwrap();

    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("Git-Warp Doctor"));
    assert!(stdout.contains("Git repository"));
    assert!(output_contains_path(&stdout, temp_dir.path()), "{stdout}");
    assert!(stdout.contains("Git binary"), "{stdout}");
    assert!(stdout.contains("git version"), "{stdout}");
    assert!(stdout.contains("Worktree base path"));
    assert!(stdout.contains("Next steps"));
}

#[cfg(unix)]
#[test]
fn test_doctor_reports_missing_git_binary_when_not_on_path() {
    let temp_dir = setup_test_repo();
    let home_dir = tempdir().unwrap();
    let empty_path_dir = tempdir().unwrap();

    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("PATH", empty_path_dir.path())
        .env("GIT_WARP_DOCTOR_PROBE_DIRS", empty_path_dir.path())
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(stdout.contains("Git binary"), "{stdout}");
    assert!(stdout.contains("git not found on PATH"), "{stdout}");
    assert!(
        stdout.contains("Install git (https://git-scm.com/downloads)"),
        "{stdout}"
    );
}

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

#[test]
fn test_config_edit_creates_config_and_launches_editor() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    let home_dir = tempdir().unwrap();
    let config_path = expected_config_path(home_dir.path());
    let marker_path = home_dir.path().join("editor-marker.txt");
    let editor_path = write_fake_editor(&home_dir.path().join("fake-editor"), &marker_path);

    let mut command = warp_command(repo_path);
    command
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("EDITOR", &editor_path)
        .env_remove("VISUAL");
    let output = command.args(["config", "--edit"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(config_path.exists());
    assert_eq!(
        normalized_path_text(fs::read_to_string(&marker_path).unwrap().trim()),
        normalized_path_text(config_path.display().to_string())
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
    assert!(stdout.contains("complete -F _warp_completion warp"));
    assert!(stdout.contains("warp __complete branches \"$cur\""));
}

#[test]
fn test_complete_branches_outputs_local_branches_matching_prefix() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    run_git(repo_path, &["branch", "261-autocomplete"]);
    run_git(repo_path, &["branch", "261-other"]);
    run_git(repo_path, &["branch", "feature/261-nested"]);

    let output = warp_command(repo_path)
        .args(["__complete", "branches", "261"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("261-autocomplete"));
    assert!(stdout.contains("261-other"));
    assert!(!stdout.contains("feature/261-nested"));
    assert!(!stdout.contains("main"));
}

#[test]
fn test_shell_config_zsh_outputs_branch_completion_for_root_and_switch() {
    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["shell-config", "zsh"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("warp_cd()"));
    assert!(stdout.contains("compdef _warp_completion warp"));
    assert!(stdout.contains("warp __complete branches \"$PREFIX\""));
    assert!(stdout.contains("CURRENT == 2"));
    assert!(stdout.contains("${words[2]} == switch && CURRENT == 3"));
}

#[test]
fn test_shell_config_fish_outputs_branch_completion_for_root_and_switch() {
    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["shell-config", "fish"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("function warp_cd"));
    assert!(stdout.contains("__fish_use_subcommand"));
    assert!(stdout.contains("__fish_seen_subcommand_from switch"));
    assert!(stdout.contains("warp __complete branches (commandline -ct)"));
}

// Keep in sync with the public variants of `Commands` in src/cli.rs.
// The binary builds the live list dynamically via clap::CommandFactory, so
// adding a new public subcommand updates the rendered snippet automatically;
// this constant exists so the test fails loudly if anyone hides a command,
// renames one, or drops an alias without updating the test alongside the enum.
const PUBLIC_SUBCOMMAND_NAMES: &[&str] = &[
    "switch",
    "ls",
    "list",
    "cleanup",
    "config",
    "agents",
    "doctor",
    "release-check",
    "hooks-install",
    "hooks-remove",
    "hooks-status",
    "shell-config",
];

#[test]
fn test_shell_config_lists_every_public_subcommand() {
    for shell in ["bash", "zsh", "fish"] {
        let output = Command::new(env!("CARGO_BIN_EXE_warp"))
            .args(["shell-config", shell])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "shell-config {shell} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        for name in PUBLIC_SUBCOMMAND_NAMES {
            assert!(
                stdout.contains(name),
                "shell-config {shell} snippet is missing subcommand `{name}`:\n{stdout}"
            );
        }
    }
}

#[cfg(unix)]
fn uninstall_script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("uninstall.sh")
}

#[cfg(unix)]
fn write_fake_warp_binary(path: &Path, version_label: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_executable(path, &format!("#!/bin/sh\necho \"{version_label}\"\n"));
}

#[cfg(unix)]
#[test]
fn test_installer_lists_existing_installs_before_replacing() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();

    write_fake_warp_binary(&install_dir.path().join("warp"), "warp 0.1.0");
    write_fake_warp_binary(
        &home_dir.path().join(".cargo").join("bin").join("warp"),
        "warp 0.1.0-cargo",
    );

    write_executable(
        &fake_bin.path().join("uname"),
        r#"#!/bin/sh
if [ "$1" = "-s" ]; then
  echo Linux
else
  echo x86_64
fi
"#,
    );
    write_executable(
        &fake_bin.path().join("curl"),
        r#"#!/bin/sh
url=""
output=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) shift; output="$1" ;;
    -O) shift; output="$1" ;;
    http*|https*) url="$1" ;;
  esac
  shift
done
case "$url" in
  *.sha256)
    archive_name="${url##*/}"
    archive_name="${archive_name%.sha256}"
    printf 'b5d54c39e66671c9731b9f471e585d8262cd4f54963f0c93082d8dcf334d4c78  %s\n' "$archive_name" > "$output"
    ;;
  *)
    printf fake > "$output"
    ;;
esac
"#,
    );
    write_executable(
        &fake_bin.path().join("tar"),
        r#"#!/bin/sh
dest=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-C" ]; then
    shift
    dest="$1"
  fi
  shift
done
cat > "$dest/warp" <<'SCRIPT'
#!/bin/sh
echo "warp 9.9.9"
SCRIPT
chmod 755 "$dest/warp"
"#,
    );

    let output = installer_command(fake_bin.path(), home_dir.path())
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains("Existing Git-Warp installs detected:"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("{}/warp", install_dir.path().display())),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("{}/.cargo/bin/warp", home_dir.path().display())),
        "{stdout}"
    );
    assert!(stdout.contains("uninstall.sh"), "{stdout}");
    assert!(stdout.contains("cargo uninstall git-warp"), "{stdout}");
}

#[cfg(unix)]
#[test]
fn test_installer_warns_when_active_warp_shadows_new_install() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();
    let shadow_dir = tempdir().unwrap();

    write_fake_warp_binary(&shadow_dir.path().join("warp"), "warp 0.0.1-old");

    write_executable(
        &fake_bin.path().join("uname"),
        r#"#!/bin/sh
if [ "$1" = "-s" ]; then
  echo Linux
else
  echo x86_64
fi
"#,
    );
    write_executable(
        &fake_bin.path().join("curl"),
        r#"#!/bin/sh
url=""
output=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) shift; output="$1" ;;
    -O) shift; output="$1" ;;
    http*|https*) url="$1" ;;
  esac
  shift
done
case "$url" in
  *.sha256)
    archive_name="${url##*/}"
    archive_name="${archive_name%.sha256}"
    printf 'b5d54c39e66671c9731b9f471e585d8262cd4f54963f0c93082d8dcf334d4c78  %s\n' "$archive_name" > "$output"
    ;;
  *)
    printf fake > "$output"
    ;;
esac
"#,
    );
    write_executable(
        &fake_bin.path().join("tar"),
        r#"#!/bin/sh
dest=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-C" ]; then
    shift
    dest="$1"
  fi
  shift
done
cat > "$dest/warp" <<'SCRIPT'
#!/bin/sh
echo "warp 9.9.9"
SCRIPT
chmod 755 "$dest/warp"
"#,
    );

    let original_path = std::env::var("PATH").unwrap_or_default();
    let output = Command::new("/bin/sh")
        .arg(install_script_path())
        .env("HOME", home_dir.path())
        .env(
            "PATH",
            format!(
                "{}:{}:{original_path}",
                shadow_dir.path().display(),
                fake_bin.path().display()
            ),
        )
        .env("GIT_WARP_VERSION", "v9.9.9")
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains(&format!(
            "Note: 'warp' on PATH resolves to {}/warp",
            shadow_dir.path().display()
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("{}/warp", install_dir.path().display())),
        "{stdout}"
    );
    assert!(stdout.contains("Reorder PATH"), "{stdout}");
}

#[cfg(unix)]
#[test]
fn test_uninstaller_removes_default_install_and_lists_others() {
    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();
    let cargo_bin = home_dir.path().join(".cargo").join("bin");

    write_fake_warp_binary(&install_dir.path().join("warp"), "warp 9.9.9");
    write_fake_warp_binary(&cargo_bin.join("warp"), "warp 9.9.9-cargo");

    let original_path = std::env::var("PATH").unwrap_or_default();
    let output = Command::new("/bin/sh")
        .arg(uninstall_script_path())
        .env("HOME", home_dir.path())
        .env("PATH", format!("{}:{original_path}", cargo_bin.display()))
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains(&format!("Removed {}/warp", install_dir.path().display())),
        "{stdout}"
    );
    assert!(
        stdout.contains("Other Git-Warp installs detected"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("{}/warp", cargo_bin.display())),
        "{stdout}"
    );
    assert!(stdout.contains("cargo uninstall git-warp"), "{stdout}");
    assert!(
        stdout.contains(&format!(
            "'warp' is still on PATH at {}/warp",
            cargo_bin.display()
        )),
        "{stdout}"
    );
    assert!(!install_dir.path().join("warp").exists());
    assert!(cargo_bin.join("warp").exists());
}

#[cfg(unix)]
#[test]
fn test_uninstaller_dry_run_keeps_default_install() {
    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();
    let target = install_dir.path().join("warp");

    write_fake_warp_binary(&target, "warp 9.9.9");

    let output = Command::new("/bin/sh")
        .arg(uninstall_script_path())
        .arg("--dry-run")
        .env("HOME", home_dir.path())
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains(&format!("Would remove {}", target.display())),
        "{stdout}"
    );
    assert!(target.exists());
}

#[cfg(unix)]
#[test]
fn test_uninstaller_reports_when_no_default_install_exists() {
    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();

    let output = Command::new("/bin/sh")
        .arg(uninstall_script_path())
        .env("HOME", home_dir.path())
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains(&format!(
            "No Git-Warp binary found at {}/warp",
            install_dir.path().display()
        )),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn test_doctor_install_check_warns_on_multiple_warp_binaries() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let probe_a = tempdir().unwrap();
    let probe_b = tempdir().unwrap();

    write_fake_warp_binary(&probe_a.path().join("warp"), "warp 0.1.0");
    write_fake_warp_binary(&probe_b.path().join("warp"), "warp 0.2.0");

    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("PATH", probe_a.path())
        .env(
            "GIT_WARP_DOCTOR_PROBE_DIRS",
            format!("{}:{}", probe_a.path().display(), probe_b.path().display()),
        )
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(stdout.contains("Install:"), "{stdout}");
    assert!(
        stdout.contains("multiple warp binaries detected"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("{}/warp (active)", probe_a.path().display())),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("{}/warp", probe_b.path().display())),
        "{stdout}"
    );
    assert!(stdout.contains("Resolve install conflicts"), "{stdout}");
}

#[cfg(unix)]
#[test]
fn test_doctor_install_check_passes_with_single_active_binary() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let probe = tempdir().unwrap();

    write_fake_warp_binary(&probe.path().join("warp"), "warp 0.3.0");

    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("PATH", probe.path())
        .env("GIT_WARP_DOCTOR_PROBE_DIRS", probe.path())
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(stdout.contains("Install:"), "{stdout}");
    assert!(
        stdout.contains(&format!("{}/warp (active)", probe.path().display())),
        "{stdout}"
    );
    assert!(
        !stdout.contains("multiple warp binaries detected"),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn test_doctor_shell_check_warns_when_install_dir_not_in_path() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let other_dir = tempdir().unwrap();

    let install_dir = home_dir.path().join(".local").join("bin");
    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("PATH", other_dir.path())
        .env("GIT_WARP_DOCTOR_PROBE_DIRS", other_dir.path())
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(stdout.contains("Default install path"), "{stdout}");
    assert!(
        stdout.contains(&format!("{} is not in PATH", install_dir.display())),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("export PATH=\"{}:$PATH\"", install_dir.display())),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn test_doctor_shell_check_passes_when_install_dir_in_path() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let install_dir = home_dir.path().join(".local").join("bin");
    fs::create_dir_all(&install_dir).unwrap();
    write_fake_warp_binary(&install_dir.join("warp"), "warp 0.4.0");

    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("PATH", &install_dir)
        .env("GIT_WARP_DOCTOR_PROBE_DIRS", &install_dir)
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(stdout.contains("Shell PATH"), "{stdout}");
    assert!(
        stdout.contains(&format!("active warp at {}/warp", install_dir.display())),
        "{stdout}"
    );
    assert!(!stdout.contains("is not in PATH"), "{stdout}");
}

#[cfg(unix)]
#[test]
fn test_doctor_shell_check_warns_when_path_shadows_install_dir() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let install_dir = home_dir.path().join(".local").join("bin");
    fs::create_dir_all(&install_dir).unwrap();
    write_fake_warp_binary(&install_dir.join("warp"), "warp 0.4.0");

    let other_dir = tempdir().unwrap();
    write_fake_warp_binary(&other_dir.path().join("warp"), "warp 0.1.0");

    let path_value = format!("{}:{}", other_dir.path().display(), install_dir.display());
    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("PATH", &path_value)
        .env("GIT_WARP_DOCTOR_PROBE_DIRS", &path_value)
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(stdout.contains("Default install path"), "{stdout}");
    assert!(
        stdout.contains("a different warp resolves first at"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "Reorder PATH to put `{}` before `{}`",
            install_dir.display(),
            other_dir.path().display()
        )),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn test_doctor_shell_check_warns_when_shell_rc_lacks_warp_cd() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let probe = tempdir().unwrap();
    write_fake_warp_binary(&probe.path().join("warp"), "warp 0.4.0");

    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("PATH", probe.path())
        .env("GIT_WARP_DOCTOR_PROBE_DIRS", probe.path())
        .env("SHELL", "/bin/zsh")
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(stdout.contains("Shell integration"), "{stdout}");
    let zshrc = home_dir.path().join(".zshrc");
    assert!(
        stdout.contains(&format!("warp_cd helper not found in {}", zshrc.display())),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "Run `warp shell-config zsh` and append the output to {}",
            zshrc.display()
        )),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn test_doctor_shell_check_passes_when_shell_rc_has_warp_cd() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let probe = tempdir().unwrap();
    write_fake_warp_binary(&probe.path().join("warp"), "warp 0.4.0");
    let zshrc = home_dir.path().join(".zshrc");
    fs::write(
        &zshrc,
        "# user config\nwarp_cd() { eval \"$(warp --terminal echo \"$@\")\"; }\n",
    )
    .unwrap();

    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("PATH", probe.path())
        .env("GIT_WARP_DOCTOR_PROBE_DIRS", probe.path())
        .env("SHELL", "/bin/zsh")
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains(&format!("warp_cd helper detected in {}", zshrc.display())),
        "{stdout}"
    );
    assert!(!stdout.contains("warp_cd helper not found"), "{stdout}");
}

#[cfg(unix)]
#[test]
fn test_doctor_shell_check_handles_unknown_shell() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let probe = tempdir().unwrap();
    write_fake_warp_binary(&probe.path().join("warp"), "warp 0.4.0");

    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("PATH", probe.path())
        .env("GIT_WARP_DOCTOR_PROBE_DIRS", probe.path())
        .env("SHELL", "/usr/bin/tcsh")
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains("unsupported shell `/usr/bin/tcsh`"),
        "{stdout}"
    );
    assert!(stdout.contains("supported: bash, zsh, fish"), "{stdout}");
}

fn setup_repo_with_origin() -> (tempfile::TempDir, tempfile::TempDir) {
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

#[test]
fn test_debug_flag_enables_debug_logging() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    let output = warp_command(repo_path)
        .env_remove("RUST_LOG")
        .args(["--debug", "ls"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stderr.contains("Debug logging enabled"),
        "--debug should emit debug-level logs on stderr: {stderr}"
    );
}

#[test]
fn test_default_logger_stays_quiet_without_debug_flag() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    let output = warp_command(repo_path)
        .env_remove("RUST_LOG")
        .arg("ls")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        !stderr.contains("Debug logging enabled"),
        "default filter should suppress debug logs: {stderr}"
    );
}

#[test]
fn test_existing_rust_log_overrides_debug_flag_default() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    let output = warp_command(repo_path)
        .env("RUST_LOG", "error")
        .args(["--debug", "ls"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        !stderr.contains("Debug logging enabled"),
        "explicit RUST_LOG=error should win over --debug: {stderr}"
    );
}
