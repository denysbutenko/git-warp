use git_warp::release::{
    ReleaseVersion, collect_metadata_checks, read_package_version, resolve_version,
};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_resolve_version() {
    // Explicit version with v-prefix
    let v = resolve_version(Some("v0.3.0"), "0.2.0").unwrap();
    assert_eq!(v.number, "0.3.0");
    assert_eq!(v.tag, "v0.3.0");

    // Explicit version without v-prefix
    let v = resolve_version(Some("0.3.0"), "0.2.0").unwrap();
    assert_eq!(v.number, "0.3.0");
    assert_eq!(v.tag, "v0.3.0");

    // Fallback to cargo version
    let v = resolve_version(None, "0.4.0").unwrap();
    assert_eq!(v.number, "0.4.0");
    assert_eq!(v.tag, "v0.4.0");

    // Empty version error
    let err = resolve_version(Some(""), "0.1.0").unwrap_err();
    assert_eq!(err.to_string(), "release version cannot be empty");

    // Trimming
    let v = resolve_version(Some("  v0.3.0  "), "0.2.0").unwrap();
    assert_eq!(v.number, "0.3.0");
}

#[test]
fn test_read_package_version() {
    let temp = tempdir().unwrap();
    let cargo_toml = r#"
[package]
name = "git-warp"
version = "0.3.1"
"#;
    fs::write(temp.path().join("Cargo.toml"), cargo_toml).unwrap();

    let version = read_package_version(temp.path()).unwrap();
    assert_eq!(version, "0.3.1");
}

#[test]
fn test_read_package_version_missing_fails() {
    let temp = tempdir().unwrap();
    let err = read_package_version(temp.path()).unwrap_err();
    assert!(err.to_string().contains("failed to read"));
}

#[test]
fn test_collect_metadata_checks_all_pass() {
    let temp = tempdir().unwrap();
    let version = "0.3.0";
    let tag = "v0.3.0";
    let expected = ReleaseVersion {
        number: version.to_string(),
        tag: tag.to_string(),
    };

    // 1. CHANGELOG.md with heading
    fs::write(
        temp.path().join("CHANGELOG.md"),
        format!("## {tag} - 2026-05-23\n"),
    )
    .unwrap();

    // 2. docs/releases/v0.3.0.md
    let releases_dir = temp.path().join("docs").join("releases");
    fs::create_dir_all(&releases_dir).unwrap();
    fs::write(releases_dir.join(format!("{tag}.md")), "notes").unwrap();

    // 3. docs/install.md with GIT_WARP_VERSION
    let docs_dir = temp.path().join("docs");
    fs::create_dir_all(&docs_dir).unwrap();
    fs::write(
        docs_dir.join("install.md"),
        format!("GIT_WARP_VERSION={tag}"),
    )
    .unwrap();

    // 4. install.sh with env override + GitHub API lookup
    fs::write(
        temp.path().join("install.sh"),
        "version=\"${GIT_WARP_VERSION:-}\"\n\
         api_url=\"https://api.github.com/repos/denysbutenko/git-warp/releases/latest\"\n",
    )
    .unwrap();

    // 5. install.ps1 + uninstall.ps1 presence
    fs::write(temp.path().join("install.ps1"), "param()").unwrap();
    fs::write(temp.path().join("uninstall.ps1"), "param()").unwrap();

    let checks = collect_metadata_checks(temp.path(), &expected, version).unwrap();

    assert_eq!(checks.len(), 7);
    for check in &checks {
        assert!(check.ok, "check failed: {} - {}", check.label, check.detail);
    }
}

#[test]
fn test_collect_metadata_checks_failures() {
    let temp = tempdir().unwrap();
    let expected = ReleaseVersion {
        number: "0.3.0".to_string(),
        tag: "v0.3.0".to_string(),
    };

    // All files missing or mismatched
    let checks = collect_metadata_checks(temp.path(), &expected, "0.2.0").unwrap();

    assert_eq!(checks.len(), 7);
    for check in &checks {
        assert!(!check.ok, "check should have failed: {}", check.label);
    }

    // Verify specific failure details
    assert!(
        checks[0]
            .detail
            .contains("Cargo.toml package version is 0.2.0, expected 0.3.0")
    );
    assert!(
        checks[1]
            .detail
            .contains("CHANGELOG.md is missing a v0.3.0 release heading")
    );
    assert!(checks[2].detail.contains("v0.3.0.md is missing"));
    assert!(
        checks[3]
            .detail
            .contains("docs/install.md does not mention GIT_WARP_VERSION=v0.3.0")
    );
    assert!(
        checks[4]
            .detail
            .contains("api.github.com/repos/.../releases/latest")
    );
    assert_eq!(checks[5].label, "install.ps1");
    assert!(checks[5].detail.contains("install.ps1 is missing"));
    assert_eq!(checks[6].label, "uninstall.ps1");
    assert!(checks[6].detail.contains("uninstall.ps1 is missing"));
}

#[test]
fn test_install_script_partial_lookup_fails() {
    let temp = tempdir().unwrap();
    let expected = ReleaseVersion {
        number: "0.3.0".to_string(),
        tag: "v0.3.0".to_string(),
    };

    // install.sh keeps env override but lost the API lookup line.
    fs::write(
        temp.path().join("install.sh"),
        "version=\"${GIT_WARP_VERSION:-v0.3.0}\"\n",
    )
    .unwrap();

    let checks = collect_metadata_checks(temp.path(), &expected, "0.3.0").unwrap();
    assert!(
        !checks[4].ok,
        "install.sh missing API lookup should fail the check"
    );
    assert_eq!(checks[4].label, "install.sh");
}

#[test]
fn test_powershell_scripts_partial_presence_fails() {
    let temp = tempdir().unwrap();
    let expected = ReleaseVersion {
        number: "0.3.0".to_string(),
        tag: "v0.3.0".to_string(),
    };

    // install.ps1 present, uninstall.ps1 missing — release must fail.
    fs::write(temp.path().join("install.ps1"), "param()").unwrap();

    let checks = collect_metadata_checks(temp.path(), &expected, "0.3.0").unwrap();

    assert_eq!(checks.len(), 7);
    assert_eq!(checks[5].label, "install.ps1");
    assert!(checks[5].ok, "install.ps1 presence check should pass");
    assert_eq!(checks[6].label, "uninstall.ps1");
    assert!(!checks[6].ok, "uninstall.ps1 absence should fail the check");
    assert!(checks[6].detail.contains("uninstall.ps1 is missing"));
}

#[test]
fn test_changelog_heading_variants() {
    let temp = tempdir().unwrap();
    let expected = ReleaseVersion {
        number: "0.3.0".to_string(),
        tag: "v0.3.0".to_string(),
    };

    // Case 1: Trailing space
    fs::write(temp.path().join("CHANGELOG.md"), "## v0.3.0 ").unwrap();
    let checks = collect_metadata_checks(temp.path(), &expected, "0.3.0").unwrap();
    assert!(checks[1].ok, "failed on trailing space");

    // Case 2: Trailing newline
    fs::write(temp.path().join("CHANGELOG.md"), "## v0.3.0\n").unwrap();
    let checks = collect_metadata_checks(temp.path(), &expected, "0.3.0").unwrap();
    assert!(checks[1].ok, "failed on trailing newline");

    // Case 3: Middle of file
    fs::write(
        temp.path().join("CHANGELOG.md"),
        "## Unreleased\n\n## v0.3.0 \n\nContent",
    )
    .unwrap();
    let checks = collect_metadata_checks(temp.path(), &expected, "0.3.0").unwrap();
    assert!(checks[1].ok, "failed in middle of file");

    // Case 4: End of file without trailing characters
    fs::write(temp.path().join("CHANGELOG.md"), "## v0.3.0").unwrap();
    let checks = collect_metadata_checks(temp.path(), &expected, "0.3.0").unwrap();
    assert!(checks[1].ok, "failed at end of file without trailing chars");

    // Case 5: Missing (should fail)
    fs::write(temp.path().join("CHANGELOG.md"), "## v0.2.0\n").unwrap();
    let checks = collect_metadata_checks(temp.path(), &expected, "0.3.0").unwrap();
    assert!(!checks[1].ok, "should fail on wrong version");
}

#[test]
fn test_collect_metadata_checks_partial_success() {
    let temp = tempdir().unwrap();
    let version = "0.3.0";
    let tag = "v0.3.0";
    let expected = ReleaseVersion {
        number: version.to_string(),
        tag: tag.to_string(),
    };

    // Only Cargo.toml version matches (it's passed as an argument to the function)
    // All other files are missing.
    let checks = collect_metadata_checks(temp.path(), &expected, version).unwrap();

    assert_eq!(checks.len(), 7);
    assert!(checks[0].ok, "Cargo.toml check should pass");
    assert!(!checks[1].ok, "CHANGELOG.md check should fail");
    assert!(!checks[2].ok, "release notes check should fail");
    assert!(!checks[3].ok, "docs/install.md check should fail");
    assert!(!checks[4].ok, "install.sh check should fail");
    assert!(!checks[5].ok, "install.ps1 check should fail");
    assert!(!checks[6].ok, "uninstall.ps1 check should fail");
}
