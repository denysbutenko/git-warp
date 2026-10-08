use std::process::Command;

#[test]
fn test_unknown_terminal_flag_value_is_rejected() {
    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["--terminal", "nonsense", "switch", "foo"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "warp should reject unknown terminal mode; stderr: {stderr}"
    );
    assert!(
        stderr.contains("tab")
            && stderr.contains("window")
            && stderr.contains("inplace")
            && stderr.contains("echo")
            && stderr.contains("current"),
        "stderr should list supported terminal modes; got: {stderr}"
    );
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
