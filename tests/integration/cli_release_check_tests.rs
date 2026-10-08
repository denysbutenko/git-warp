use crate::integration::cli_surface_support::normalized_path_text;
use std::process::Command;

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
        .args(["release-check", "--metadata-only", "--version", "v0.7.0"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "{stdout}");
    assert!(
        stderr.contains("Cargo.toml package version is 0.6.0, expected 0.7.0"),
        "{stderr}"
    );
    assert!(
        normalized_path_text(&stderr).contains("docs/releases/v0.7.0.md is missing"),
        "{stderr}"
    );
}
