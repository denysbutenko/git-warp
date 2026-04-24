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
    fs::write(repo_path.join("package.json"), r#"{"name":"test-repo"}"#).unwrap();
    fs::write(repo_path.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n").unwrap();

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

fn create_fake_pnpm(bin_dir: &Path, script_body: &str) -> PathBuf {
    let pnpm_path = bin_dir.join("pnpm");
    fs::write(&pnpm_path, script_body).unwrap();
    #[cfg(unix)]
    make_executable(&pnpm_path);
    pnpm_path
}

fn run_warp_switch(repo_path: &Path, branch: &str, path_env: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["--terminal", "echo", "switch", "--no-cow", branch])
        .current_dir(repo_path)
        .env("PATH", path_env)
        .output()
        .unwrap()
}

#[test]
fn test_warp_switch_runs_pnpm_install_only_for_new_worktree() {
    let temp_dir = setup_git_repo();
    let repo_path = temp_dir.path();
    let bin_dir = tempdir().unwrap();
    let marker_path = temp_dir.path().join("pnpm-runs.txt");

    create_fake_pnpm(
        bin_dir.path(),
        &format!(
            "#!/bin/sh\nprintf \"%s\\n\" \"$PWD\" >> \"{}\"\nexit 0\n",
            marker_path.display()
        ),
    );

    let original_path = std::env::var("PATH").unwrap_or_default();
    let path_env = format!("{}:{}", bin_dir.path().display(), original_path);

    let first_run = run_warp_switch(repo_path, "feature/pnpm-once", &path_env);
    assert!(first_run.status.success());
    assert!(
        String::from_utf8_lossy(&first_run.stdout)
            .contains("Detected pnpm repo, ran `pnpm install`")
    );

    let second_run = run_warp_switch(repo_path, "feature/pnpm-once", &path_env);
    assert!(second_run.status.success());
    assert!(
        !String::from_utf8_lossy(&second_run.stdout)
            .contains("Detected pnpm repo, ran `pnpm install`")
    );

    let marker_contents = fs::read_to_string(marker_path).unwrap();
    assert_eq!(marker_contents.lines().count(), 1);
}

#[test]
fn test_warp_switch_warns_when_pnpm_install_fails_but_still_succeeds() {
    let temp_dir = setup_git_repo();
    let repo_path = temp_dir.path();
    let bin_dir = tempdir().unwrap();

    create_fake_pnpm(
        bin_dir.path(),
        "#!/bin/sh\necho \"install failed\" >&2\nexit 1\n",
    );

    let original_path = std::env::var("PATH").unwrap_or_default();
    let path_env = format!("{}:{}", bin_dir.path().display(), original_path);

    let output = run_warp_switch(repo_path, "feature/pnpm-warn", &path_env);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Detected pnpm repo but `pnpm install` failed: install failed"));
    assert!(stdout.contains("Worktree creation: created"));
}
