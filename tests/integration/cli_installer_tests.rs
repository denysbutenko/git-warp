#![cfg(unix)]

use crate::integration::cli_surface_support::{
    create_worktree, install_script_path, installer_command, setup_test_repo,
    uninstall_script_path, write_executable, write_fake_warp_binary,
};
use std::process::Command;
use tempfile::tempdir;

#[cfg(unix)]
#[test]
fn test_installer_explains_unsupported_platform() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();

    write_executable(
        &fake_bin.path().join("uname"),
        r#"#!/bin/sh
if [ "$1" = "-s" ]; then
  echo Plan9
else
  echo sparc
fi
"#,
    );

    let output = installer_command(fake_bin.path(), home_dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains("unsupported operating system: Plan9"),
        "{stderr}"
    );
    assert!(stderr.contains("Supported prebuilt targets:"), "{stderr}");
    assert!(stderr.contains("GIT_WARP_INSTALL_METHOD=cargo"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn test_installer_explains_unsupported_architecture() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();

    write_executable(
        &fake_bin.path().join("uname"),
        r#"#!/bin/sh
if [ "$1" = "-s" ]; then
  echo Darwin
else
  echo powerpc
fi
"#,
    );

    let output = installer_command(fake_bin.path(), home_dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains("unsupported CPU architecture: powerpc"),
        "{stderr}"
    );
    assert!(stderr.contains("Supported prebuilt targets:"), "{stderr}");
    assert!(stderr.contains("GIT_WARP_INSTALL_METHOD=cargo"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn test_installer_explains_failed_binary_download() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();

    write_executable(
        &fake_bin.path().join("uname"),
        r#"#!/bin/sh
if [ "$1" = "-s" ]; then
  echo Darwin
else
  echo arm64
fi
"#,
    );
    write_executable(
        &fake_bin.path().join("curl"),
        r#"#!/bin/sh
echo "not found" >&2
exit 22
"#,
    );

    let output = installer_command(fake_bin.path(), home_dir.path())
        .env("GIT_WARP_DOWNLOAD_BASE", "https://example.invalid/releases")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains(
            "failed to download https://example.invalid/releases/git-warp-v9.9.9-aarch64-apple-darwin.tar.gz"
        ),
        "{stderr}"
    );
    assert!(
        stderr.contains("The release asset may not exist yet for this platform or version."),
        "{stderr}"
    );
    assert!(stderr.contains("GIT_WARP_INSTALL_METHOD=cargo"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn test_installer_explains_cargo_fallback_failure() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();

    write_executable(
        &fake_bin.path().join("cargo"),
        r#"#!/bin/sh
echo "cargo exploded" >&2
exit 101
"#,
    );

    let output = installer_command(fake_bin.path(), home_dir.path())
        .env("GIT_WARP_INSTALL_METHOD", "cargo")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("cargo exploded"), "{stderr}");
    assert!(
        stderr.contains("Cargo install failed for Git-Warp v9.9.9."),
        "{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn test_installer_prints_actionable_path_guidance() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();

    write_executable(
        &fake_bin.path().join("uname"),
        r#"#!/bin/sh
if [ "$1" = "-s" ]; then
  echo Linux
else
  echo x86_64
fi
"#,
    );
    write_executable(
        &fake_bin.path().join("curl"),
        r#"#!/bin/sh
url=""
output=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) shift; output="$1" ;;
    -O) shift; output="$1" ;;
    http*|https*) url="$1" ;;
  esac
  shift
done
case "$url" in
  *.sha256)
    archive_name="${url##*/}"
    archive_name="${archive_name%.sha256}"
    printf 'b5d54c39e66671c9731b9f471e585d8262cd4f54963f0c93082d8dcf334d4c78  %s\n' "$archive_name" > "$output"
    ;;
  *)
    printf fake > "$output"
    ;;
esac
"#,
    );
    write_executable(
        &fake_bin.path().join("tar"),
        r#"#!/bin/sh
dest=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-C" ]; then
    shift
    dest="$1"
  fi
  shift
done
cat > "$dest/warp" <<'SCRIPT'
#!/bin/sh
echo warp 9.9.9
SCRIPT
chmod 755 "$dest/warp"
"#,
    );

    let output = installer_command(fake_bin.path(), home_dir.path())
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(stdout.contains("warp 9.9.9"), "{stdout}");
    assert!(
        stdout.contains(&format!(
            "Add {} to PATH so your shell can find 'warp':",
            install_dir.path().display()
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "export PATH=\"{}:$PATH\"",
            install_dir.path().display()
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains("Open a new terminal or run 'warp doctor' after updating PATH."),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn test_installer_fails_on_checksum_mismatch() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();

    write_executable(
        &fake_bin.path().join("uname"),
        r#"#!/bin/sh
if [ "$1" = "-s" ]; then
  echo Linux
else
  echo x86_64
fi
"#,
    );
    write_executable(
        &fake_bin.path().join("curl"),
        r#"#!/bin/sh
url=""
output=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) shift; output="$1" ;;
    -O) shift; output="$1" ;;
    http*|https*) url="$1" ;;
  esac
  shift
done
case "$url" in
  *.sha256)
    archive_name="${url##*/}"
    archive_name="${archive_name%.sha256}"
    printf '0000000000000000000000000000000000000000000000000000000000000000  %s\n' "$archive_name" > "$output"
    ;;
  *)
    printf fake > "$output"
    ;;
esac
"#,
    );

    let output = installer_command(fake_bin.path(), home_dir.path())
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains(
            "checksum verification failed for git-warp-v9.9.9-x86_64-unknown-linux-gnu.tar.gz"
        ),
        "{stderr}"
    );
    assert!(
        stderr
            .contains("expected: 0000000000000000000000000000000000000000000000000000000000000000"),
        "{stderr}"
    );
    assert!(
        stderr
            .contains("actual:   b5d54c39e66671c9731b9f471e585d8262cd4f54963f0c93082d8dcf334d4c78"),
        "{stderr}"
    );
    assert!(stderr.contains("GIT_WARP_INSTALL_METHOD=cargo"), "{stderr}");
    assert!(
        !install_dir.path().join("warp").exists(),
        "warp binary should not be installed on checksum mismatch"
    );
}

#[cfg(unix)]
#[test]
fn test_installer_skips_checksum_when_opted_out() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();

    write_executable(
        &fake_bin.path().join("uname"),
        r#"#!/bin/sh
if [ "$1" = "-s" ]; then
  echo Linux
else
  echo x86_64
fi
"#,
    );
    write_executable(
        &fake_bin.path().join("curl"),
        r#"#!/bin/sh
output=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-o" ]; then
    shift
    output="$1"
  fi
  shift
done
printf fake > "$output"
"#,
    );
    write_executable(
        &fake_bin.path().join("tar"),
        r#"#!/bin/sh
dest=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-C" ]; then
    shift
    dest="$1"
  fi
  shift
done
cat > "$dest/warp" <<'SCRIPT'
#!/bin/sh
echo warp 9.9.9
SCRIPT
chmod 755 "$dest/warp"
"#,
    );

    let output = installer_command(fake_bin.path(), home_dir.path())
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .env("GIT_WARP_SKIP_CHECKSUM", "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains("Skipping checksum verification"),
        "{stdout}"
    );
    assert!(!stderr.contains("checksum verification failed"), "{stderr}");
    assert!(install_dir.path().join("warp").exists());
}

#[cfg(unix)]
#[test]
fn test_installer_lists_existing_installs_before_replacing() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();

    write_fake_warp_binary(&install_dir.path().join("warp"), "warp 0.1.0");
    write_fake_warp_binary(
        &home_dir.path().join(".cargo").join("bin").join("warp"),
        "warp 0.1.0-cargo",
    );

    write_executable(
        &fake_bin.path().join("uname"),
        r#"#!/bin/sh
if [ "$1" = "-s" ]; then
  echo Linux
else
  echo x86_64
fi
"#,
    );
    write_executable(
        &fake_bin.path().join("curl"),
        r#"#!/bin/sh
url=""
output=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) shift; output="$1" ;;
    -O) shift; output="$1" ;;
    http*|https*) url="$1" ;;
  esac
  shift
done
case "$url" in
  *.sha256)
    archive_name="${url##*/}"
    archive_name="${archive_name%.sha256}"
    printf 'b5d54c39e66671c9731b9f471e585d8262cd4f54963f0c93082d8dcf334d4c78  %s\n' "$archive_name" > "$output"
    ;;
  *)
    printf fake > "$output"
    ;;
esac
"#,
    );
    write_executable(
        &fake_bin.path().join("tar"),
        r#"#!/bin/sh
dest=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-C" ]; then
    shift
    dest="$1"
  fi
  shift
done
cat > "$dest/warp" <<'SCRIPT'
#!/bin/sh
echo "warp 9.9.9"
SCRIPT
chmod 755 "$dest/warp"
"#,
    );

    let output = installer_command(fake_bin.path(), home_dir.path())
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains("Existing Git-Warp installs detected:"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("{}/warp", install_dir.path().display())),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("{}/.cargo/bin/warp", home_dir.path().display())),
        "{stdout}"
    );
    assert!(stdout.contains("uninstall.sh"), "{stdout}");
    assert!(stdout.contains("cargo uninstall git-warp"), "{stdout}");
}

#[cfg(unix)]
#[test]
fn test_installer_warns_when_active_warp_shadows_new_install() {
    let fake_bin = tempdir().unwrap();
    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();
    let shadow_dir = tempdir().unwrap();

    write_fake_warp_binary(&shadow_dir.path().join("warp"), "warp 0.0.1-old");

    write_executable(
        &fake_bin.path().join("uname"),
        r#"#!/bin/sh
if [ "$1" = "-s" ]; then
  echo Linux
else
  echo x86_64
fi
"#,
    );
    write_executable(
        &fake_bin.path().join("curl"),
        r#"#!/bin/sh
url=""
output=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) shift; output="$1" ;;
    -O) shift; output="$1" ;;
    http*|https*) url="$1" ;;
  esac
  shift
done
case "$url" in
  *.sha256)
    archive_name="${url##*/}"
    archive_name="${archive_name%.sha256}"
    printf 'b5d54c39e66671c9731b9f471e585d8262cd4f54963f0c93082d8dcf334d4c78  %s\n' "$archive_name" > "$output"
    ;;
  *)
    printf fake > "$output"
    ;;
esac
"#,
    );
    write_executable(
        &fake_bin.path().join("tar"),
        r#"#!/bin/sh
dest=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-C" ]; then
    shift
    dest="$1"
  fi
  shift
done
cat > "$dest/warp" <<'SCRIPT'
#!/bin/sh
echo "warp 9.9.9"
SCRIPT
chmod 755 "$dest/warp"
"#,
    );

    let original_path = std::env::var("PATH").unwrap_or_default();
    let output = Command::new("/bin/sh")
        .arg(install_script_path())
        .env("HOME", home_dir.path())
        .env(
            "PATH",
            format!(
                "{}:{}:{original_path}",
                shadow_dir.path().display(),
                fake_bin.path().display()
            ),
        )
        .env("GIT_WARP_VERSION", "v9.9.9")
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains(&format!(
            "Note: 'warp' on PATH resolves to {}/warp",
            shadow_dir.path().display()
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("{}/warp", install_dir.path().display())),
        "{stdout}"
    );
    assert!(stdout.contains("Reorder PATH"), "{stdout}");
}

#[cfg(unix)]
#[test]
fn test_uninstaller_removes_default_install_and_lists_others() {
    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();
    let cargo_bin = home_dir.path().join(".cargo").join("bin");

    write_fake_warp_binary(&install_dir.path().join("warp"), "warp 9.9.9");
    write_fake_warp_binary(&cargo_bin.join("warp"), "warp 9.9.9-cargo");

    let original_path = std::env::var("PATH").unwrap_or_default();
    let output = Command::new("/bin/sh")
        .arg(uninstall_script_path())
        .env("HOME", home_dir.path())
        .env("PATH", format!("{}:{original_path}", cargo_bin.display()))
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains(&format!("Removed {}/warp", install_dir.path().display())),
        "{stdout}"
    );
    assert!(
        stdout.contains("Other Git-Warp installs detected"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("{}/warp", cargo_bin.display())),
        "{stdout}"
    );
    assert!(stdout.contains("cargo uninstall git-warp"), "{stdout}");
    assert!(
        stdout.contains(&format!(
            "'warp' is still on PATH at {}/warp",
            cargo_bin.display()
        )),
        "{stdout}"
    );
    assert!(!install_dir.path().join("warp").exists());
    assert!(cargo_bin.join("warp").exists());
}

#[cfg(unix)]
#[test]
fn test_uninstaller_dry_run_keeps_default_install() {
    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();
    let target = install_dir.path().join("warp");

    write_fake_warp_binary(&target, "warp 9.9.9");

    let output = Command::new("/bin/sh")
        .arg(uninstall_script_path())
        .arg("--dry-run")
        .env("HOME", home_dir.path())
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains(&format!("Would remove {}", target.display())),
        "{stdout}"
    );
    assert!(target.exists());
}

#[cfg(unix)]
#[test]
fn test_uninstaller_dry_run_lists_project_hooks_inside_linked_worktree() {
    let primary = setup_test_repo();
    let worktree_path = create_worktree(primary.path(), "feature-203");

    assert!(
        worktree_path.join(".git").is_file(),
        ".git in a linked worktree should be a file, not a directory"
    );

    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();
    let target = install_dir.path().join("warp");
    write_fake_warp_binary(&target, "warp 9.9.9");

    let output = Command::new("/bin/sh")
        .arg(uninstall_script_path())
        .arg("--dry-run")
        .current_dir(&worktree_path)
        .env("HOME", home_dir.path())
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(stdout.contains("Would remove user hooks with:"), "{stdout}");
    assert!(
        stdout.contains("Would remove project hooks with:"),
        "{stdout}"
    );
    assert!(target.exists());
}

#[cfg(unix)]
#[test]
fn test_uninstaller_reports_when_no_default_install_exists() {
    let home_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();

    let output = Command::new("/bin/sh")
        .arg(uninstall_script_path())
        .env("HOME", home_dir.path())
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("GIT_WARP_INSTALL_DIR", install_dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains(&format!(
            "No Git-Warp binary found at {}/warp",
            install_dir.path().display()
        )),
        "{stdout}"
    );
}
