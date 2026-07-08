use git_warp::release::{
    ReleaseVersion, collect_metadata_checks, read_package_version, resolve_version,
};
use git_warp::warp_executable_name;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_warp_executable_name_matches_platform() {
    let name = warp_executable_name();
    if cfg!(windows) {
        assert_eq!(name, "warp.exe");
    } else {
        assert_eq!(name, "warp");
    }
}

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

    // 4. install.sh with env override + GitHub API lookup + checksum guards
    fs::write(
        temp.path().join("install.sh"),
        "version=\"${GIT_WARP_VERSION:-}\"\n\
         api_url=\"https://api.github.com/repos/denysbutenko/git-warp/releases/latest\"\n\
         if [ \"${GIT_WARP_SKIP_CHECKSUM:-0}\" != \"1\" ]; then sha256sum -c \"$sums\"; fi\n",
    )
    .unwrap();

    // 5. install.ps1 + uninstall.ps1 + uninstall.sh with release-critical content
    fs::write(
        temp.path().join("install.ps1"),
        "$api = \"https://api.github.com/repos/denysbutenko/git-warp/releases/latest\"\n\
         $envVersion = [Environment]::GetEnvironmentVariable('GIT_WARP_VERSION')\n\
         if ($envVersion) { $tag = $env:GIT_WARP_VERSION }\n\
         $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash\n\
         $skipEnv = [Environment]::GetEnvironmentVariable('GIT_WARP_SKIP_CHECKSUM')\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("uninstall.ps1"),
        "$installRoot = Coalesce $InstallRoot 'GIT_WARP_INSTALL_ROOT' $null\n\
         & $Target hooks-remove --level user --runtime all *> $null\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("uninstall.sh"),
        "\"$target\" hooks-remove --level user --runtime all >/dev/null 2>&1\n",
    )
    .unwrap();

    // 6. src/cli.rs with the PowerShell shell-config emitter
    let src_dir = temp.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(
        src_dir.join("cli.rs"),
        "// function warp_cd emitter\n\
         // Register-ArgumentCompleter -CommandName warp -Native\n",
    )
    .unwrap();

    let checks = collect_metadata_checks(temp.path(), &expected, version).unwrap();

    assert_eq!(checks.len(), 9);
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

    assert_eq!(checks.len(), 9);
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
    assert_eq!(checks[4].label, "install.sh");
    assert!(
        checks[4]
            .detail
            .contains("api.github.com/repos/.../releases/latest")
    );
    assert!(
        checks[4]
            .detail
            .contains("shasum -a 256 -c or sha256sum -c checksum verification")
    );
    assert!(
        checks[4]
            .detail
            .contains("GIT_WARP_SKIP_CHECKSUM opt-out guard")
    );
    assert_eq!(checks[5].label, "install.ps1");
    assert!(
        checks[5]
            .detail
            .contains("Get-FileHash -Algorithm SHA256 verification")
    );
    assert!(checks[5].detail.contains("missing: "));
    assert_eq!(checks[6].label, "uninstall.ps1");
    assert!(
        checks[6]
            .detail
            .contains("warp hooks-remove --level user invocation")
    );
    assert_eq!(checks[7].label, "uninstall.sh");
    assert!(
        checks[7]
            .detail
            .contains("warp hooks-remove --level user invocation")
    );
    assert_eq!(checks[8].label, "src/cli.rs");
    assert!(checks[8].detail.contains("PowerShell shell-config emitter"));
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
fn test_install_ps1_missing_checksum_fails() {
    let temp = tempdir().unwrap();
    let expected = ReleaseVersion {
        number: "0.3.0".to_string(),
        tag: "v0.3.0".to_string(),
    };

    // install.ps1 keeps env override + API lookup but lost the checksum step.
    fs::write(
        temp.path().join("install.ps1"),
        "$api = \"https://api.github.com/repos/denysbutenko/git-warp/releases/latest\"\n\
         $tag = $env:GIT_WARP_VERSION\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("uninstall.ps1"),
        "$installRoot = Coalesce $InstallRoot 'GIT_WARP_INSTALL_ROOT' $null\n\
         & $Target hooks-remove --level user --runtime all *> $null\n",
    )
    .unwrap();

    let checks = collect_metadata_checks(temp.path(), &expected, "0.3.0").unwrap();

    assert_eq!(checks[5].label, "install.ps1");
    assert!(
        !checks[5].ok,
        "install.ps1 without checksum guard should fail the check"
    );
    assert!(
        checks[5].detail.contains("Get-FileHash checksum step"),
        "fail detail should call out the missing checksum, got: {}",
        checks[5].detail
    );
    assert!(
        checks[5].detail.contains("SHA256 algorithm flag"),
        "fail detail should call out the missing SHA256 flag, got: {}",
        checks[5].detail
    );
    assert!(checks[6].ok, "uninstall.ps1 guard should still pass");
}

#[test]
fn test_install_ps1_missing_skip_checksum_fails() {
    let temp = tempdir().unwrap();
    let expected = ReleaseVersion {
        number: "0.3.0".to_string(),
        tag: "v0.3.0".to_string(),
    };

    // install.ps1 keeps env override + API lookup + Get-FileHash SHA256 but lost the SkipChecksum opt-out.
    fs::write(
        temp.path().join("install.ps1"),
        "$api = \"https://api.github.com/repos/denysbutenko/git-warp/releases/latest\"\n\
         $tag = $env:GIT_WARP_VERSION\n\
         $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("uninstall.ps1"),
        "$installRoot = Coalesce $InstallRoot 'GIT_WARP_INSTALL_ROOT' $null\n\
         & $Target hooks-remove --level user --runtime all *> $null\n",
    )
    .unwrap();

    let checks = collect_metadata_checks(temp.path(), &expected, "0.3.0").unwrap();

    assert_eq!(checks[5].label, "install.ps1");
    assert!(
        !checks[5].ok,
        "install.ps1 without SkipChecksum opt-out should fail the check"
    );
    assert!(
        checks[5]
            .detail
            .contains("GIT_WARP_SKIP_CHECKSUM / -SkipChecksum opt-out"),
        "fail detail should call out the missing SkipChecksum opt-out, got: {}",
        checks[5].detail
    );
    assert!(checks[6].ok, "uninstall.ps1 guard should still pass");
}

#[test]
fn test_uninstall_ps1_missing_hooks_remove_fails() {
    let temp = tempdir().unwrap();
    let expected = ReleaseVersion {
        number: "0.3.0".to_string(),
        tag: "v0.3.0".to_string(),
    };

    fs::write(
        temp.path().join("install.ps1"),
        "$api = \"https://api.github.com/repos/denysbutenko/git-warp/releases/latest\"\n\
         $tag = $env:GIT_WARP_VERSION\n\
         $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash\n\
         $skipEnv = [Environment]::GetEnvironmentVariable('GIT_WARP_SKIP_CHECKSUM')\n",
    )
    .unwrap();
    // uninstall.ps1 forgot the hooks-remove call.
    fs::write(
        temp.path().join("uninstall.ps1"),
        "$installRoot = Coalesce $InstallRoot 'GIT_WARP_INSTALL_ROOT' $null\n\
         Remove-Item -LiteralPath $target -Force\n",
    )
    .unwrap();

    let checks = collect_metadata_checks(temp.path(), &expected, "0.3.0").unwrap();

    assert!(checks[5].ok, "install.ps1 guard should pass");
    assert_eq!(checks[6].label, "uninstall.ps1");
    assert!(
        !checks[6].ok,
        "uninstall.ps1 without hooks-remove should fail"
    );
    assert!(
        checks[6].detail.contains("user-level hooks-remove call"),
        "fail detail should call out the missing hooks-remove, got: {}",
        checks[6].detail
    );
}

#[test]
fn test_uninstall_ps1_missing_installroot_fails() {
    let temp = tempdir().unwrap();
    let expected = ReleaseVersion {
        number: "0.3.0".to_string(),
        tag: "v0.3.0".to_string(),
    };

    fs::write(
        temp.path().join("install.ps1"),
        "$api = \"https://api.github.com/repos/denysbutenko/git-warp/releases/latest\"\n\
         $tag = $env:GIT_WARP_VERSION\n\
         $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash\n\
         $skipEnv = [Environment]::GetEnvironmentVariable('GIT_WARP_SKIP_CHECKSUM')\n",
    )
    .unwrap();
    // uninstall.ps1 keeps hooks-remove but lost the InstallRoot handling.
    fs::write(
        temp.path().join("uninstall.ps1"),
        "& $Target hooks-remove --level user --runtime all *> $null\n",
    )
    .unwrap();

    let checks = collect_metadata_checks(temp.path(), &expected, "0.3.0").unwrap();

    assert!(checks[5].ok, "install.ps1 guard should pass");
    assert_eq!(checks[6].label, "uninstall.ps1");
    assert!(
        !checks[6].ok,
        "uninstall.ps1 without InstallRoot handling should fail"
    );
    assert!(
        checks[6]
            .detail
            .contains("GIT_WARP_INSTALL_ROOT / -InstallRoot handling"),
        "fail detail should call out the missing InstallRoot handling, got: {}",
        checks[6].detail
    );
}

#[test]
fn test_cli_rs_missing_powershell_emitter_fails() {
    let temp = tempdir().unwrap();
    let expected = ReleaseVersion {
        number: "0.3.0".to_string(),
        tag: "v0.3.0".to_string(),
    };

    // src/cli.rs kept warp_cd but lost the Register-ArgumentCompleter line.
    let src_dir = temp.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(
        src_dir.join("cli.rs"),
        "fn handle_shell_config() { println!(\"function warp_cd\"); }\n",
    )
    .unwrap();

    let checks = collect_metadata_checks(temp.path(), &expected, "0.3.0").unwrap();

    assert_eq!(checks[8].label, "src/cli.rs");
    assert!(
        !checks[8].ok,
        "src/cli.rs without Register-ArgumentCompleter should fail"
    );
    assert!(
        checks[8]
            .detail
            .contains("Register-ArgumentCompleter block"),
        "fail detail should call out the missing completer, got: {}",
        checks[8].detail
    );
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

    assert_eq!(checks.len(), 9);
    assert!(checks[0].ok, "Cargo.toml check should pass");
    assert!(!checks[1].ok, "CHANGELOG.md check should fail");
    assert!(!checks[2].ok, "release notes check should fail");
    assert!(!checks[3].ok, "docs/install.md check should fail");
    assert!(!checks[4].ok, "install.sh check should fail");
    assert!(!checks[5].ok, "install.ps1 check should fail");
    assert!(!checks[6].ok, "uninstall.ps1 check should fail");
    assert!(!checks[7].ok, "uninstall.sh check should fail");
    assert!(!checks[8].ok, "src/cli.rs check should fail");
}

#[test]
fn test_install_script_missing_checksum_verifier_fails() {
    let temp = tempdir().unwrap();
    let expected = ReleaseVersion {
        number: "0.3.0".to_string(),
        tag: "v0.3.0".to_string(),
    };

    // install.sh keeps env override + API lookup + skip env, but lost the shasum/sha256sum call.
    fs::write(
        temp.path().join("install.sh"),
        "version=\"${GIT_WARP_VERSION:-}\"\n\
         api_url=\"https://api.github.com/repos/denysbutenko/git-warp/releases/latest\"\n\
         if [ \"${GIT_WARP_SKIP_CHECKSUM:-0}\" != \"1\" ]; then :; fi\n",
    )
    .unwrap();

    let checks = collect_metadata_checks(temp.path(), &expected, "0.3.0").unwrap();

    assert_eq!(checks[4].label, "install.sh");
    assert!(
        !checks[4].ok,
        "install.sh without shasum/sha256sum should fail"
    );
    assert!(
        checks[4]
            .detail
            .contains("shasum -a 256 -c or sha256sum -c checksum verification"),
        "fail detail should call out the missing checksum verifier, got: {}",
        checks[4].detail
    );
}

#[test]
fn test_install_script_missing_skip_env_fails() {
    let temp = tempdir().unwrap();
    let expected = ReleaseVersion {
        number: "0.3.0".to_string(),
        tag: "v0.3.0".to_string(),
    };

    // install.sh keeps env override + API lookup + sha256sum -c, but lost the skip env guard.
    fs::write(
        temp.path().join("install.sh"),
        "version=\"${GIT_WARP_VERSION:-}\"\n\
         api_url=\"https://api.github.com/repos/denysbutenko/git-warp/releases/latest\"\n\
         sha256sum -c \"$sums\"\n",
    )
    .unwrap();

    let checks = collect_metadata_checks(temp.path(), &expected, "0.3.0").unwrap();

    assert_eq!(checks[4].label, "install.sh");
    assert!(
        !checks[4].ok,
        "install.sh without GIT_WARP_SKIP_CHECKSUM should fail"
    );
    assert!(
        checks[4]
            .detail
            .contains("GIT_WARP_SKIP_CHECKSUM opt-out guard"),
        "fail detail should call out the missing skip env, got: {}",
        checks[4].detail
    );
}

#[test]
fn test_uninstall_script_missing_hooks_remove_fails() {
    let temp = tempdir().unwrap();
    let expected = ReleaseVersion {
        number: "0.3.0".to_string(),
        tag: "v0.3.0".to_string(),
    };

    // install.sh + install.ps1 + uninstall.ps1 satisfy their guards.
    fs::write(
        temp.path().join("install.sh"),
        "version=\"${GIT_WARP_VERSION:-}\"\n\
         api_url=\"https://api.github.com/repos/denysbutenko/git-warp/releases/latest\"\n\
         if [ \"${GIT_WARP_SKIP_CHECKSUM:-0}\" != \"1\" ]; then sha256sum -c \"$sums\"; fi\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("install.ps1"),
        "$api = \"https://api.github.com/repos/denysbutenko/git-warp/releases/latest\"\n\
         $tag = $env:GIT_WARP_VERSION\n\
         $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash\n\
         $skipEnv = [Environment]::GetEnvironmentVariable('GIT_WARP_SKIP_CHECKSUM')\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("uninstall.ps1"),
        "param([string]$InstallRoot = $env:GIT_WARP_INSTALL_ROOT)\n\
         & $Target hooks-remove --level user --runtime all *> $null\n",
    )
    .unwrap();
    // uninstall.sh forgot the hooks-remove call.
    fs::write(temp.path().join("uninstall.sh"), "rm -f \"$target\"\n").unwrap();

    let checks = collect_metadata_checks(temp.path(), &expected, "0.3.0").unwrap();

    assert!(checks[4].ok, "install.sh guard should pass");
    assert!(checks[5].ok, "install.ps1 guard should pass");
    assert!(checks[6].ok, "uninstall.ps1 guard should pass");
    assert_eq!(checks[7].label, "uninstall.sh");
    assert!(
        !checks[7].ok,
        "uninstall.sh without hooks-remove should fail"
    );
    assert!(
        checks[7].detail.contains("user-level hooks-remove call"),
        "fail detail should call out the missing hooks-remove, got: {}",
        checks[7].detail
    );
}
