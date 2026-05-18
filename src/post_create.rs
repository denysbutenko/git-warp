use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    Pnpm,
    Yarn,
    Bun,
    Npm,
}

impl PackageManager {
    pub fn binary(self) -> &'static str {
        match self {
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
            PackageManager::Bun => "bun",
            PackageManager::Npm => "npm",
        }
    }

    pub fn install_label(self) -> &'static str {
        match self {
            PackageManager::Pnpm => "pnpm install",
            PackageManager::Yarn => "yarn install",
            PackageManager::Bun => "bun install",
            PackageManager::Npm => "npm install",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostCreateSetupStatus {
    SkippedExistingWorktree,
    SkippedDisabled,
    SkippedNoLockfile,
    Installed(PackageManager),
    Warned {
        manager: PackageManager,
        reason: String,
    },
}

const LOCKFILES: &[(&str, PackageManager)] = &[
    ("pnpm-lock.yaml", PackageManager::Pnpm),
    ("yarn.lock", PackageManager::Yarn),
    ("bun.lock", PackageManager::Bun),
    ("bun.lockb", PackageManager::Bun),
    ("package-lock.json", PackageManager::Npm),
];

pub fn run_post_create_setup<P: AsRef<Path>>(
    worktree_path: P,
    newly_created: bool,
    auto_install: bool,
) -> PostCreateSetupStatus {
    run_post_create_setup_with_resolver(worktree_path.as_ref(), newly_created, auto_install, |m| {
        PathBuf::from(m.binary())
    })
}

fn run_post_create_setup_with_resolver<F>(
    worktree_path: &Path,
    newly_created: bool,
    auto_install: bool,
    resolve_binary: F,
) -> PostCreateSetupStatus
where
    F: Fn(PackageManager) -> PathBuf,
{
    if !newly_created {
        return PostCreateSetupStatus::SkippedExistingWorktree;
    }

    if !auto_install {
        return PostCreateSetupStatus::SkippedDisabled;
    }

    let Some(manager) = detect_manager(worktree_path) else {
        return PostCreateSetupStatus::SkippedNoLockfile;
    };

    let binary = resolve_binary(manager);
    match Command::new(&binary)
        .arg("install")
        .current_dir(worktree_path)
        .output()
    {
        Ok(output) if output.status.success() => PostCreateSetupStatus::Installed(manager),
        Ok(output) => PostCreateSetupStatus::Warned {
            manager,
            reason: command_failure_message(manager, &output),
        },
        Err(error) => PostCreateSetupStatus::Warned {
            manager,
            reason: error.to_string(),
        },
    }
}

fn detect_manager(worktree_path: &Path) -> Option<PackageManager> {
    LOCKFILES
        .iter()
        .find_map(|(lockfile, manager)| worktree_path.join(lockfile).is_file().then_some(*manager))
}

fn command_failure_message(manager: PackageManager, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return stdout;
    }

    match output.status.code() {
        Some(code) => format!("{} exited with status {}", manager.install_label(), code),
        None => format!("{} terminated by signal", manager.install_label()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    fn write_marker_script(path: &Path, marker: &Path) {
        fs::write(
            path,
            format!(
                "#!/bin/sh\nprintf \"%s\" \"$PWD\" > \"{}\"\nexit 0\n",
                marker.display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        make_executable(path);
    }

    fn write_failing_script(path: &Path) {
        fs::write(path, "#!/bin/sh\necho \"install failed\" >&2\nexit 1\n").unwrap();
        #[cfg(unix)]
        make_executable(path);
    }

    fn resolver_for(manager: PackageManager, path: PathBuf) -> impl Fn(PackageManager) -> PathBuf {
        move |m| {
            assert_eq!(m, manager);
            path.clone()
        }
    }

    fn write_package_json(dir: &Path) {
        fs::write(dir.join("package.json"), r#"{"name":"test-repo"}"#).unwrap();
    }

    #[test]
    fn test_skips_existing_worktree() {
        let temp = tempdir().unwrap();
        write_package_json(temp.path());
        fs::write(temp.path().join("pnpm-lock.yaml"), "").unwrap();

        let status = run_post_create_setup_with_resolver(temp.path(), false, true, |m| {
            PathBuf::from(format!("/missing/{}", m.binary()))
        });

        assert_eq!(status, PostCreateSetupStatus::SkippedExistingWorktree);
    }

    #[test]
    fn test_skips_when_auto_install_disabled() {
        let temp = tempdir().unwrap();
        write_package_json(temp.path());
        fs::write(temp.path().join("pnpm-lock.yaml"), "").unwrap();

        let status = run_post_create_setup_with_resolver(temp.path(), true, false, |m| {
            PathBuf::from(format!("/missing/{}", m.binary()))
        });

        assert_eq!(status, PostCreateSetupStatus::SkippedDisabled);
    }

    #[test]
    fn test_skips_when_no_lockfile() {
        let temp = tempdir().unwrap();
        write_package_json(temp.path());

        let status = run_post_create_setup_with_resolver(temp.path(), true, true, |m| {
            PathBuf::from(format!("/missing/{}", m.binary()))
        });

        assert_eq!(status, PostCreateSetupStatus::SkippedNoLockfile);
    }

    #[test]
    fn test_runs_pnpm_when_pnpm_lockfile_present() {
        let temp = tempdir().unwrap();
        write_package_json(temp.path());
        fs::write(temp.path().join("pnpm-lock.yaml"), "").unwrap();

        let bin = temp.path().join("fake-pnpm");
        let marker = temp.path().join("ran.txt");
        write_marker_script(&bin, &marker);

        let status = run_post_create_setup_with_resolver(
            temp.path(),
            true,
            true,
            resolver_for(PackageManager::Pnpm, bin),
        );

        assert_eq!(
            status,
            PostCreateSetupStatus::Installed(PackageManager::Pnpm)
        );
        let recorded = fs::canonicalize(fs::read_to_string(marker).unwrap()).unwrap();
        let expected = fs::canonicalize(temp.path()).unwrap();
        assert_eq!(recorded, expected);
    }

    #[test]
    fn test_runs_yarn_when_yarn_lockfile_present() {
        let temp = tempdir().unwrap();
        write_package_json(temp.path());
        fs::write(temp.path().join("yarn.lock"), "").unwrap();

        let bin = temp.path().join("fake-yarn");
        let marker = temp.path().join("ran.txt");
        write_marker_script(&bin, &marker);

        let status = run_post_create_setup_with_resolver(
            temp.path(),
            true,
            true,
            resolver_for(PackageManager::Yarn, bin),
        );

        assert_eq!(
            status,
            PostCreateSetupStatus::Installed(PackageManager::Yarn)
        );
    }

    #[test]
    fn test_runs_bun_when_bun_lockfile_present() {
        let temp = tempdir().unwrap();
        write_package_json(temp.path());
        fs::write(temp.path().join("bun.lockb"), "").unwrap();

        let bin = temp.path().join("fake-bun");
        let marker = temp.path().join("ran.txt");
        write_marker_script(&bin, &marker);

        let status = run_post_create_setup_with_resolver(
            temp.path(),
            true,
            true,
            resolver_for(PackageManager::Bun, bin),
        );

        assert_eq!(
            status,
            PostCreateSetupStatus::Installed(PackageManager::Bun)
        );
    }

    #[test]
    fn test_runs_bun_when_text_bun_lockfile_present() {
        let temp = tempdir().unwrap();
        write_package_json(temp.path());
        fs::write(temp.path().join("bun.lock"), "").unwrap();

        let bin = temp.path().join("fake-bun");
        let marker = temp.path().join("ran.txt");
        write_marker_script(&bin, &marker);

        let status = run_post_create_setup_with_resolver(
            temp.path(),
            true,
            true,
            resolver_for(PackageManager::Bun, bin),
        );

        assert_eq!(
            status,
            PostCreateSetupStatus::Installed(PackageManager::Bun)
        );
    }

    #[test]
    fn test_bun_text_lockfile_wins_over_legacy_binary() {
        let temp = tempdir().unwrap();
        write_package_json(temp.path());
        fs::write(temp.path().join("bun.lock"), "").unwrap();
        fs::write(temp.path().join("bun.lockb"), "").unwrap();

        let bin = temp.path().join("fake-bun");
        let marker = temp.path().join("ran.txt");
        write_marker_script(&bin, &marker);

        let status = run_post_create_setup_with_resolver(
            temp.path(),
            true,
            true,
            resolver_for(PackageManager::Bun, bin),
        );

        assert_eq!(
            status,
            PostCreateSetupStatus::Installed(PackageManager::Bun)
        );

        let bun_lock_pos = LOCKFILES
            .iter()
            .position(|(name, _)| *name == "bun.lock")
            .expect("bun.lock present in LOCKFILES");
        let bun_lockb_pos = LOCKFILES
            .iter()
            .position(|(name, _)| *name == "bun.lockb")
            .expect("bun.lockb present in LOCKFILES");
        assert!(
            bun_lock_pos < bun_lockb_pos,
            "bun.lock must be detected before bun.lockb"
        );
    }

    #[test]
    fn test_runs_npm_when_npm_lockfile_present() {
        let temp = tempdir().unwrap();
        write_package_json(temp.path());
        fs::write(temp.path().join("package-lock.json"), "").unwrap();

        let bin = temp.path().join("fake-npm");
        let marker = temp.path().join("ran.txt");
        write_marker_script(&bin, &marker);

        let status = run_post_create_setup_with_resolver(
            temp.path(),
            true,
            true,
            resolver_for(PackageManager::Npm, bin),
        );

        assert_eq!(
            status,
            PostCreateSetupStatus::Installed(PackageManager::Npm)
        );
    }

    #[test]
    fn test_pnpm_wins_over_other_lockfiles() {
        let temp = tempdir().unwrap();
        write_package_json(temp.path());
        fs::write(temp.path().join("pnpm-lock.yaml"), "").unwrap();
        fs::write(temp.path().join("yarn.lock"), "").unwrap();
        fs::write(temp.path().join("bun.lockb"), "").unwrap();
        fs::write(temp.path().join("package-lock.json"), "").unwrap();

        let bin = temp.path().join("fake-pnpm");
        let marker = temp.path().join("ran.txt");
        write_marker_script(&bin, &marker);

        let status = run_post_create_setup_with_resolver(
            temp.path(),
            true,
            true,
            resolver_for(PackageManager::Pnpm, bin),
        );

        assert_eq!(
            status,
            PostCreateSetupStatus::Installed(PackageManager::Pnpm)
        );
    }

    #[test]
    fn test_warns_on_failed_install() {
        let temp = tempdir().unwrap();
        write_package_json(temp.path());
        fs::write(temp.path().join("yarn.lock"), "").unwrap();

        let bin = temp.path().join("fake-yarn");
        write_failing_script(&bin);

        let status = run_post_create_setup_with_resolver(
            temp.path(),
            true,
            true,
            resolver_for(PackageManager::Yarn, bin),
        );

        assert_eq!(
            status,
            PostCreateSetupStatus::Warned {
                manager: PackageManager::Yarn,
                reason: "install failed".to_string(),
            }
        );
    }
}
