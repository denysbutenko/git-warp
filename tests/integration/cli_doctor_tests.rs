use crate::integration::cli_surface_support::{
    output_contains_path, setup_test_repo, warp_bin_name, warp_command, write_fake_warp_binary,
};
use std::fs;
use tempfile::tempdir;

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
fn test_doctor_install_check_warns_on_multiple_warp_binaries() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let probe_a = tempdir().unwrap();
    let probe_b = tempdir().unwrap();

    write_fake_warp_binary(&probe_a.path().join(warp_bin_name()), "warp 0.1.0");
    write_fake_warp_binary(&probe_b.path().join(warp_bin_name()), "warp 0.2.0");

    let probe_dirs = std::env::join_paths([probe_a.path(), probe_b.path()]).unwrap();
    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("PATH", probe_a.path())
        .env("GIT_WARP_DOCTOR_PROBE_DIRS", probe_dirs)
        .env_remove("PSModulePath")
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let active_path = probe_a.path().join(warp_bin_name());
    let inactive_path = probe_b.path().join(warp_bin_name());

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(stdout.contains("Install:"), "{stdout}");
    assert!(
        stdout.contains("multiple warp binaries detected"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("{} (active)", active_path.display())),
        "{stdout}"
    );
    assert!(
        stdout.contains(&inactive_path.display().to_string()),
        "{stdout}"
    );
    assert!(stdout.contains("Resolve install conflicts"), "{stdout}");
}

#[test]
fn test_doctor_install_check_passes_with_single_active_binary() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let probe = tempdir().unwrap();

    write_fake_warp_binary(&probe.path().join(warp_bin_name()), "warp 0.3.0");

    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("PATH", probe.path())
        .env("GIT_WARP_DOCTOR_PROBE_DIRS", probe.path())
        .env_remove("PSModulePath")
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let active_path = probe.path().join(warp_bin_name());

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(stdout.contains("Install:"), "{stdout}");
    assert!(
        stdout.contains(&format!("{} (active)", active_path.display())),
        "{stdout}"
    );
    assert!(
        !stdout.contains("multiple warp binaries detected"),
        "{stdout}"
    );
}

#[test]
fn test_doctor_shell_check_warns_when_install_dir_not_in_path() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let other_dir = tempdir().unwrap();
    let install_dir = home_dir.path().join("install-bin");

    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("PATH", other_dir.path())
        .env("GIT_WARP_DOCTOR_PROBE_DIRS", other_dir.path())
        .env("GIT_WARP_DEFAULT_INSTALL_DIR", &install_dir)
        .env_remove("PSModulePath")
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
        stdout.contains(&format!("Add `{}` to PATH", install_dir.display())),
        "{stdout}"
    );
}

#[test]
fn test_doctor_shell_check_passes_when_install_dir_in_path() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let install_dir = home_dir.path().join("install-bin");
    fs::create_dir_all(&install_dir).unwrap();
    write_fake_warp_binary(&install_dir.join(warp_bin_name()), "warp 0.4.0");

    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("PATH", &install_dir)
        .env("GIT_WARP_DOCTOR_PROBE_DIRS", &install_dir)
        .env("GIT_WARP_DEFAULT_INSTALL_DIR", &install_dir)
        .env_remove("PSModulePath")
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let active_path = install_dir.join(warp_bin_name());

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(stdout.contains("Shell PATH"), "{stdout}");
    assert!(
        stdout.contains(&format!("active warp at {}", active_path.display())),
        "{stdout}"
    );
    assert!(!stdout.contains("is not in PATH"), "{stdout}");
}

#[test]
fn test_doctor_shell_check_warns_when_path_shadows_install_dir() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let install_dir = home_dir.path().join("install-bin");
    fs::create_dir_all(&install_dir).unwrap();
    write_fake_warp_binary(&install_dir.join(warp_bin_name()), "warp 0.4.0");

    let other_dir = tempdir().unwrap();
    write_fake_warp_binary(&other_dir.path().join(warp_bin_name()), "warp 0.1.0");

    let path_value = std::env::join_paths([other_dir.path(), install_dir.as_path()]).unwrap();
    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("PATH", &path_value)
        .env("GIT_WARP_DOCTOR_PROBE_DIRS", &path_value)
        .env("GIT_WARP_DEFAULT_INSTALL_DIR", &install_dir)
        .env_remove("PSModulePath")
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
    assert!(
        stdout.contains("supported: bash, zsh, fish, powershell"),
        "{stdout}"
    );
    assert!(
        stdout.contains("(or use PowerShell), then run `warp shell-config <shell>`"),
        "{stdout}"
    );
}

#[cfg(windows)]
#[test]
fn test_doctor_shell_check_recognizes_powershell() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let install_dir = home_dir
        .path()
        .join("Programs")
        .join("git-warp")
        .join("bin");
    fs::create_dir_all(&install_dir).unwrap();
    write_fake_warp_binary(&install_dir.join(warp_bin_name()), "warp 0.4.0");

    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("PATH", &install_dir)
        .env("GIT_WARP_DOCTOR_PROBE_DIRS", &install_dir)
        .env("GIT_WARP_DEFAULT_INSTALL_DIR", &install_dir)
        .env("PSModulePath", "C:\\fake\\Modules")
        .env_remove("SHELL")
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains("Run `warp shell-config powershell`"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("Add `{}` to $env:PATH", install_dir.display())),
        "{stdout}"
    );
    assert!(
        !stdout.contains("Set SHELL to bash, zsh, or fish"),
        "{stdout}"
    );
}

#[cfg(windows)]
#[test]
fn test_doctor_install_check_recommends_install_ps1_when_missing() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let empty_probe = tempdir().unwrap();
    let empty_path = tempdir().unwrap();

    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("PATH", empty_path.path())
        .env("GIT_WARP_DOCTOR_PROBE_DIRS", empty_probe.path())
        .env_remove("SHELL")
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains("no warp binary found in PATH or known install dirs"),
        "{stdout}"
    );
    assert!(stdout.contains("install.ps1 | iex"), "{stdout}");
}
