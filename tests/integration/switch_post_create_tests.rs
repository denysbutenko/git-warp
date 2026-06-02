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

fn create_fake_cargo(bin_dir: &Path, marker: &Path) -> PathBuf {
    create_fake_path_shim(bin_dir, "cargo", marker, false)
}

fn create_fake_pnpm(bin_dir: &Path, marker: &Path) -> PathBuf {
    create_fake_path_shim(bin_dir, "pnpm", marker, false)
}

fn create_fake_path_shim(bin_dir: &Path, name: &str, marker: &Path, fail: bool) -> PathBuf {
    #[cfg(windows)]
    let shim_path = bin_dir.join(format!("{name}.cmd"));
    #[cfg(not(windows))]
    let shim_path = bin_dir.join(name);

    let script_body = if fail {
        #[cfg(windows)]
        {
            "@echo off\r\necho install failed 1>&2\r\nexit /b 1\r\n".to_string()
        }
        #[cfg(unix)]
        {
            "#!/bin/sh\necho \"install failed\" >&2\nexit 1\n".to_string()
        }
    } else {
        #[cfg(windows)]
        {
            format!(
                "@echo off\r\necho %CD%>>\"{}\"\r\nexit /b 0\r\n",
                marker.display()
            )
        }
        #[cfg(unix)]
        {
            format!(
                "#!/bin/sh\nprintf \"%s\\n\" \"$PWD\" >> \"{}\"\nexit 0\n",
                marker.display()
            )
        }
    };

    fs::write(&shim_path, script_body).unwrap();
    #[cfg(unix)]
    make_executable(&shim_path);
    shim_path
}

fn prepend_path_env(dir: &Path) -> String {
    let mut paths = vec![dir.to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

fn run_warp_switch(repo_path: &Path, branch: &str, path_env: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["--terminal", "echo", "switch", "--no-cow", branch])
        .current_dir(repo_path)
        .env("PATH", path_env)
        .env("GIT_WARP_POST_CREATE__AUTO_INSTALL", "true")
        .output()
        .unwrap()
}

#[test]
fn test_warp_switch_runs_pnpm_install_only_for_new_worktree() {
    let temp_dir = setup_git_repo();
    let repo_path = temp_dir.path();
    let bin_dir = tempdir().unwrap();
    let marker_path = temp_dir.path().join("pnpm-runs.txt");

    create_fake_pnpm(bin_dir.path(), &marker_path);

    let path_env = prepend_path_env(bin_dir.path());

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

    create_fake_path_shim(bin_dir.path(), "pnpm", &repo_path.join("unused"), true);

    let path_env = prepend_path_env(bin_dir.path());

    let output = run_warp_switch(repo_path, "feature/pnpm-warn", &path_env);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Detected pnpm repo but `pnpm install` failed: install failed"));
    assert!(stdout.contains("Worktree creation: created"));
}

#[test]
fn test_warp_switch_runs_cargo_check_for_rust_repo() {
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

    fs::write(repo_path.join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();

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

    let bin_dir = tempdir().unwrap();
    let marker_path = temp_dir.path().join("cargo-runs.txt");

    create_fake_cargo(bin_dir.path(), &marker_path);

    let path_env = prepend_path_env(bin_dir.path());

    let output = run_warp_switch(repo_path, "feature/cargo-check", &path_env);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Detected cargo repo, ran `cargo check`"));

    let marker_contents = fs::read_to_string(marker_path).unwrap();
    assert_eq!(marker_contents.lines().count(), 1);
}
