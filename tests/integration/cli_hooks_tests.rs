use crate::integration::cli_surface_support::{setup_test_repo, warp_command};

#[test]
fn test_hooks_install_rejects_unknown_level() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    let output = warp_command(repo_path)
        .args(["hooks-install", "--level", "garbage", "--runtime", "claude"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit when hooks-install --level is unknown; stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    for variant in ["user", "project", "console"] {
        assert!(
            stderr.contains(variant),
            "stderr should mention valid level `{variant}`: {stderr}",
        );
    }
}

#[test]
fn test_hooks_remove_rejects_unknown_level() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    let output = warp_command(repo_path)
        .args(["hooks-remove", "--level", "console", "--runtime", "claude"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit when hooks-remove --level is unknown; stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    for variant in ["user", "project"] {
        assert!(
            stderr.contains(variant),
            "stderr should mention valid level `{variant}`: {stderr}",
        );
    }
}
