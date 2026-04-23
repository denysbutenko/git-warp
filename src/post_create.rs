use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostCreateSetupStatus {
    SkippedExistingWorktree,
    SkippedNonPnpmRepo,
    Installed,
    Warned(String),
}

pub fn run_post_create_setup<P: AsRef<Path>>(
    worktree_path: P,
    newly_created: bool,
) -> PostCreateSetupStatus {
    run_post_create_setup_with_pnpm(worktree_path.as_ref(), newly_created, Path::new("pnpm"))
}

fn run_post_create_setup_with_pnpm(
    worktree_path: &Path,
    newly_created: bool,
    pnpm_path: &Path,
) -> PostCreateSetupStatus {
    if !newly_created {
        return PostCreateSetupStatus::SkippedExistingWorktree;
    }

    if !is_pnpm_repo(worktree_path) {
        return PostCreateSetupStatus::SkippedNonPnpmRepo;
    }

    match Command::new(pnpm_path)
        .arg("install")
        .current_dir(worktree_path)
        .output()
    {
        Ok(output) if output.status.success() => PostCreateSetupStatus::Installed,
        Ok(output) => PostCreateSetupStatus::Warned(command_failure_message(&output)),
        Err(error) => PostCreateSetupStatus::Warned(error.to_string()),
    }
}

fn is_pnpm_repo(worktree_path: &Path) -> bool {
    worktree_path.join("package.json").is_file() && worktree_path.join("pnpm-lock.yaml").is_file()
}

fn command_failure_message(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return stdout;
    }

    match output.status.code() {
        Some(code) => format!("pnpm install exited with status {}", code),
        None => "pnpm install terminated by signal".to_string(),
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

    fn create_pnpm_repo(path: &Path) {
        fs::write(path.join("package.json"), r#"{"name":"test-repo"}"#).unwrap();
        fs::write(path.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'").unwrap();
    }

    #[test]
    fn test_run_post_create_setup_skips_existing_worktree() {
        let temp_dir = tempdir().unwrap();
        create_pnpm_repo(temp_dir.path());

        let status =
            run_post_create_setup_with_pnpm(temp_dir.path(), false, Path::new("/missing/pnpm"));

        assert_eq!(status, PostCreateSetupStatus::SkippedExistingWorktree);
    }

    #[test]
    fn test_run_post_create_setup_skips_non_pnpm_repo() {
        let temp_dir = tempdir().unwrap();
        fs::write(
            temp_dir.path().join("package.json"),
            r#"{"name":"test-repo"}"#,
        )
        .unwrap();

        let status =
            run_post_create_setup_with_pnpm(temp_dir.path(), true, Path::new("/missing/pnpm"));

        assert_eq!(status, PostCreateSetupStatus::SkippedNonPnpmRepo);
    }

    #[test]
    fn test_run_post_create_setup_runs_pnpm_install_for_new_pnpm_repo() {
        let temp_dir = tempdir().unwrap();
        create_pnpm_repo(temp_dir.path());

        let pnpm_path = temp_dir.path().join("fake-pnpm");
        let marker_path = temp_dir.path().join("pnpm-ran.txt");
        fs::write(
            &pnpm_path,
            format!(
                "#!/bin/sh\nprintf \"%s\" \"$PWD\" > \"{}\"\nexit 0\n",
                marker_path.display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        make_executable(&pnpm_path);

        let status = run_post_create_setup_with_pnpm(temp_dir.path(), true, &pnpm_path);

        assert_eq!(status, PostCreateSetupStatus::Installed);
        let recorded_path = fs::canonicalize(fs::read_to_string(marker_path).unwrap()).unwrap();
        let expected_path = fs::canonicalize(temp_dir.path()).unwrap();
        assert_eq!(recorded_path, expected_path);
    }

    #[test]
    fn test_run_post_create_setup_warns_on_failed_install() {
        let temp_dir = tempdir().unwrap();
        create_pnpm_repo(temp_dir.path());

        let pnpm_path = temp_dir.path().join("fake-pnpm");
        fs::write(
            &pnpm_path,
            "#!/bin/sh\necho \"install failed\" >&2\nexit 1\n",
        )
        .unwrap();
        #[cfg(unix)]
        make_executable(&pnpm_path);

        let status = run_post_create_setup_with_pnpm(temp_dir.path(), true, &pnpm_path);

        assert_eq!(
            status,
            PostCreateSetupStatus::Warned("install failed".to_string())
        );
    }
}
