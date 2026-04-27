use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn setup_git_repo() -> tempfile::TempDir {
    let temp_dir = tempdir().unwrap();
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    fs::write(repo_path.join("README.md"), "# Test Repository\n").unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    temp_dir
}

fn create_fake_osascript(bin_dir: &Path, log_path: &Path) -> PathBuf {
    let script_path = bin_dir.join("osascript");
    fs::write(
        &script_path,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"-e\" ]; then\n  printf '%s\\n---\\n' \"$2\" >> \"{}\"\nfi\nexit 0\n",
            log_path.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    make_executable(&script_path);
    script_path
}

fn create_fake_open(bin_dir: &Path, log_path: &Path) -> PathBuf {
    let script_path = bin_dir.join("open");
    fs::write(
        &script_path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"{}\"\nexit 0\n",
            log_path.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    make_executable(&script_path);
    script_path
}

fn create_failing_open(bin_dir: &Path) -> PathBuf {
    let script_path = bin_dir.join("open");
    fs::write(
        &script_path,
        "#!/bin/sh\necho \"open failed\" >&2\nexit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    make_executable(&script_path);
    script_path
}

fn create_fake_shell(bin_dir: &Path, log_path: &Path) -> PathBuf {
    let script_path = bin_dir.join("fake-shell");
    fs::write(
        &script_path,
        format!(
            "#!/bin/sh\nprintf 'cwd=%s\\nargs=%s\\n' \"$(pwd)\" \"$*\" >> \"{}\"\nexit 0\n",
            log_path.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    make_executable(&script_path);
    script_path
}

fn write_config(home_dir: &Path, app: &str) {
    write_config_with_terminal_options(home_dir, app, "tab", true, &[]);
}

fn write_config_with_terminal_options(
    home_dir: &Path,
    app: &str,
    terminal_mode: &str,
    auto_activate: bool,
    init_commands: &[&str],
) {
    let config_dir = home_dir
        .join("Library")
        .join("Application Support")
        .join("git-warp");
    fs::create_dir_all(&config_dir).unwrap();
    let init_commands = init_commands
        .iter()
        .map(|command| format!("\"{command}\""))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        config_dir.join("config.toml"),
        format!(
            r#"
terminal_mode = "{terminal_mode}"
use_cow = false

[terminal]
app = "{app}"
auto_activate = {auto_activate}
init_commands = [{init_commands}]
"#
        ),
    )
    .unwrap();
}

fn run_warp_switch(
    repo_path: &Path,
    branch: &str,
    home_dir: &Path,
    path_env: &str,
    term_program: &str,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["switch", "--no-cow", branch])
        .current_dir(repo_path)
        .env("HOME", home_dir)
        .env("PATH", path_env)
        .env("TERM_PROGRAM", term_program)
        .output()
        .unwrap()
}

#[test]
fn test_warp_switch_honors_explicit_warp_terminal_app() {
    let repo_dir = setup_git_repo();
    let repo_path = repo_dir.path();
    let home_dir = tempdir().unwrap();
    let bin_dir = tempdir().unwrap();
    let osascript_log_dir = tempdir().unwrap();
    let open_log_dir = tempdir().unwrap();
    let osascript_log = osascript_log_dir.path().join("osascript.log");
    let open_log = open_log_dir.path().join("open.log");

    write_config(home_dir.path(), "warp");
    create_fake_osascript(bin_dir.path(), &osascript_log);
    create_fake_open(bin_dir.path(), &open_log);

    let original_path = std::env::var("PATH").unwrap_or_default();
    let path_env = format!("{}:{}", bin_dir.path().display(), original_path);

    let output = run_warp_switch(
        repo_path,
        "feature/warp-explicit",
        home_dir.path(),
        &path_env,
        "iTerm.app",
    );

    assert!(output.status.success());

    let osascript_contents = fs::read_to_string(&osascript_log).unwrap_or_default();
    let open_contents = fs::read_to_string(&open_log).unwrap_or_default();
    assert!(
        open_contents.contains("warp://action/new_tab?path="),
        "expected Warp URI open call, got open={}, osascript={}",
        open_contents,
        osascript_contents
    );
}

#[test]
fn test_warp_switch_reports_terminal_handoff_failure_as_incomplete() {
    let repo_dir = setup_git_repo();
    let repo_path = repo_dir.path();
    let home_dir = tempdir().unwrap();
    let bin_dir = tempdir().unwrap();
    let osascript_log_dir = tempdir().unwrap();
    let osascript_log = osascript_log_dir.path().join("osascript.log");

    write_config(home_dir.path(), "warp");
    create_fake_osascript(bin_dir.path(), &osascript_log);
    create_failing_open(bin_dir.path());

    let original_path = std::env::var("PATH").unwrap_or_default();
    let path_env = format!("{}:{}", bin_dir.path().display(), original_path);
    let worktree_path = repo_path.join("worktrees").join("handoff-fails");

    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["switch", "--no-cow", "feature/handoff-fails", "--path"])
        .arg(&worktree_path)
        .current_dir(repo_path)
        .env("HOME", home_dir.path())
        .env("PATH", path_env)
        .env("TERM_PROGRAM", "WarpTerminal")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("✅ Worktree creation: created"), "{stdout}");
    assert!(
        stdout.contains("✅ Branch checkout: feature/handoff-fails"),
        "{stdout}"
    );
    assert!(stdout.contains("⚠️  Terminal handoff: failed"), "{stdout}");
    assert!(stdout.contains("Retry with `--terminal echo`"), "{stdout}");
    assert!(stdout.contains("⚠️  Switch incomplete"), "{stdout}");
    assert!(stdout.contains("💡 Run: cd '"), "{stdout}");
}

#[test]
fn test_warp_switch_branch_already_in_use_prints_recovery_guidance() {
    let repo_dir = setup_git_repo();
    let repo_path = repo_dir.path();
    let home_dir = tempdir().unwrap();
    let worktree_path = repo_path.join("worktrees").join("main-copy");

    write_config(home_dir.path(), "auto");

    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["switch", "--no-cow", "main", "--path"])
        .arg(&worktree_path)
        .current_dir(repo_path)
        .env("HOME", home_dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("Failed to create worktree"), "{stderr}");
    assert!(
        stderr.contains("Use a different branch name or run `warp ls`"),
        "{stderr}"
    );
}

#[test]
fn test_dynamic_branch_reuses_existing_worktree_for_branch() {
    let repo_dir = setup_git_repo();
    let repo_path = repo_dir.path();
    let home_dir = tempdir().unwrap();
    let worktree_path = repo_path
        .join("external-worktrees")
        .join("feature-existing");

    write_config_with_terminal_options(home_dir.path(), "auto", "echo", true, &[]);

    let create_output = Command::new("git")
        .args(["worktree", "add", "-b", "feature/existing"])
        .arg(&worktree_path)
        .current_dir(repo_path)
        .output()
        .unwrap();
    assert!(
        create_output.status.success(),
        "{}",
        String::from_utf8_lossy(&create_output.stderr)
    );

    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["--terminal", "echo", "feature/existing"])
        .current_dir(repo_path)
        .env("HOME", home_dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let expected_path = worktree_path.canonicalize().unwrap();

    assert!(output.status.success(), "stdout={stdout}\nstderr={stderr}");
    assert!(
        stdout.contains(&format!(
            "📁 Worktree already exists at: {}",
            expected_path.display()
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains("↪️  Worktree creation: already existed"),
        "{stdout}"
    );
    assert!(
        stdout.contains("✅ Branch checkout: feature/existing"),
        "{stdout}"
    );
}

#[test]
fn test_warp_switch_reports_existing_worktree_branch_mismatch_as_incomplete() {
    let repo_dir = setup_git_repo();
    let repo_path = repo_dir.path();
    let home_dir = tempdir().unwrap();
    let bin_dir = tempdir().unwrap();
    let osascript_log_dir = tempdir().unwrap();
    let osascript_log = osascript_log_dir.path().join("osascript.log");
    let worktree_path = repo_path.join("worktrees").join("feature-existing");

    write_config(home_dir.path(), "warp");
    create_fake_osascript(bin_dir.path(), &osascript_log);

    let create_output = Command::new("git")
        .args(["worktree", "add", "-b", "feature/existing"])
        .arg(&worktree_path)
        .current_dir(repo_path)
        .output()
        .unwrap();
    assert!(
        create_output.status.success(),
        "{}",
        String::from_utf8_lossy(&create_output.stderr)
    );

    let original_path = std::env::var("PATH").unwrap_or_default();
    let path_env = format!("{}:{}", bin_dir.path().display(), original_path);

    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args([
            "--terminal",
            "echo",
            "switch",
            "--no-cow",
            "feature/wrong",
            "--path",
        ])
        .arg(&worktree_path)
        .current_dir(repo_path)
        .env("HOME", home_dir.path())
        .env("PATH", path_env)
        .env("TERM_PROGRAM", "WarpTerminal")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("↪️  Worktree creation: already existed"),
        "{stdout}"
    );
    assert!(
        stdout.contains("⚠️  Branch checkout: expected feature/wrong"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Use a different --path or run `warp ls`"),
        "{stdout}"
    );
    assert!(stdout.contains("⚠️  Switch incomplete"), "{stdout}");
    assert!(stdout.contains("💡 Run: cd '"), "{stdout}");
}

#[test]
fn test_warp_switch_current_starts_shell_in_worktree() {
    let repo_dir = setup_git_repo();
    let repo_path = repo_dir.path();
    let home_dir = tempdir().unwrap();
    let bin_dir = tempdir().unwrap();
    let shell_log_dir = tempdir().unwrap();
    let shell_log = shell_log_dir.path().join("shell.log");

    write_config(home_dir.path(), "auto");
    let fake_shell = create_fake_shell(bin_dir.path(), &shell_log);

    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args([
            "--terminal",
            "current",
            "switch",
            "--no-cow",
            "feature/warp-current",
        ])
        .current_dir(repo_path)
        .env("HOME", home_dir.path())
        .env("SHELL", &fake_shell)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected current switch to succeed, stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let shell_contents = fs::read_to_string(&shell_log).unwrap_or_default();
    assert!(
        shell_contents.contains("worktrees/feature-warp-current"),
        "expected fake shell to start in worktree, got {}",
        shell_contents
    );
}

#[test]
fn test_warp_switch_auto_prefers_current_warp_terminal() {
    let repo_dir = setup_git_repo();
    let repo_path = repo_dir.path();
    let home_dir = tempdir().unwrap();
    let bin_dir = tempdir().unwrap();
    let osascript_log_dir = tempdir().unwrap();
    let open_log_dir = tempdir().unwrap();
    let osascript_log = osascript_log_dir.path().join("osascript.log");
    let open_log = open_log_dir.path().join("open.log");

    write_config(home_dir.path(), "auto");
    create_fake_osascript(bin_dir.path(), &osascript_log);
    create_fake_open(bin_dir.path(), &open_log);

    let original_path = std::env::var("PATH").unwrap_or_default();
    let path_env = format!("{}:{}", bin_dir.path().display(), original_path);

    let output = run_warp_switch(
        repo_path,
        "feature/warp-auto",
        home_dir.path(),
        &path_env,
        "WarpTerminal",
    );

    assert!(output.status.success());

    let osascript_contents = fs::read_to_string(&osascript_log).unwrap_or_default();
    let open_contents = fs::read_to_string(&open_log).unwrap_or_default();
    assert!(
        open_contents.contains("warp://action/new_tab?path="),
        "expected Warp URI open call, got open={}, osascript={}",
        open_contents,
        osascript_contents
    );
}

#[test]
fn test_warp_switch_echo_includes_configured_init_commands() {
    let repo_dir = setup_git_repo();
    let repo_path = repo_dir.path();
    let home_dir = tempdir().unwrap();

    write_config_with_terminal_options(
        home_dir.path(),
        "auto",
        "echo",
        true,
        &["corepack enable", "pnpm install"],
    );

    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["switch", "--no-cow", "feature/init-commands"])
        .current_dir(repo_path)
        .env("HOME", home_dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("# Navigate to worktree:"), "{stdout}");
    assert!(stdout.contains("cd '"), "{stdout}");
    assert!(stdout.contains("corepack enable"), "{stdout}");
    assert!(stdout.contains("pnpm install"), "{stdout}");
}

#[cfg(target_os = "macos")]
#[test]
fn test_warp_switch_applescript_activates_when_configured() {
    let repo_dir = setup_git_repo();
    let repo_path = repo_dir.path();
    let home_dir = tempdir().unwrap();
    let bin_dir = tempdir().unwrap();
    let osascript_log_dir = tempdir().unwrap();
    let osascript_log = osascript_log_dir.path().join("osascript.log");

    write_config_with_terminal_options(home_dir.path(), "terminal", "tab", true, &[]);
    create_fake_osascript(bin_dir.path(), &osascript_log);

    let original_path = std::env::var("PATH").unwrap_or_default();
    let path_env = format!("{}:{}", bin_dir.path().display(), original_path);

    let output = run_warp_switch(
        repo_path,
        "feature/activate-terminal",
        home_dir.path(),
        &path_env,
        "Apple_Terminal",
    );

    assert!(output.status.success());

    let osascript_contents = fs::read_to_string(&osascript_log).unwrap_or_default();
    assert!(
        osascript_contents.contains("\n    activate\n")
            || osascript_contents.contains("\nactivate\n"),
        "expected activate command in AppleScript, got {osascript_contents}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_warp_switch_applescript_skips_activation_when_disabled() {
    let repo_dir = setup_git_repo();
    let repo_path = repo_dir.path();
    let home_dir = tempdir().unwrap();
    let bin_dir = tempdir().unwrap();
    let osascript_log_dir = tempdir().unwrap();
    let osascript_log = osascript_log_dir.path().join("osascript.log");

    write_config_with_terminal_options(home_dir.path(), "terminal", "tab", false, &[]);
    create_fake_osascript(bin_dir.path(), &osascript_log);

    let original_path = std::env::var("PATH").unwrap_or_default();
    let path_env = format!("{}:{}", bin_dir.path().display(), original_path);

    let output = run_warp_switch(
        repo_path,
        "feature/no-activate-terminal",
        home_dir.path(),
        &path_env,
        "Apple_Terminal",
    );

    assert!(output.status.success());

    let osascript_contents = fs::read_to_string(&osascript_log).unwrap_or_default();
    assert!(
        !osascript_contents.contains("\n    activate\n")
            && !osascript_contents.contains("\nactivate\n"),
        "did not expect activate command in AppleScript, got {osascript_contents}"
    );
}
