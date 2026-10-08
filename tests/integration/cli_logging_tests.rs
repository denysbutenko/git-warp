use crate::integration::cli_surface_support::{setup_test_repo, warp_command};

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
