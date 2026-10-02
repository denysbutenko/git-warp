use std::process::Command;

use super::{doctor_ok, doctor_warn};

pub(super) fn report_doctor_git_binary(next_steps: &mut Vec<String>) {
    use std::io::ErrorKind;

    match Command::new("git").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let line = stdout
                .lines()
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("git --version reported no output");
            doctor_ok("Git binary", line);
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = stderr
                .lines()
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("git --version exited with a non-zero status");
            doctor_warn("Git binary", format!("git --version failed: {detail}"));
            next_steps.push(
                "Reinstall git so worktree, status, and diff commands keep working.".to_string(),
            );
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            doctor_warn("Git binary", "git not found on PATH");
            next_steps.push(
                "Install git (https://git-scm.com/downloads) so warp can shell out to worktree, status, and diff commands."
                    .to_string(),
            );
        }
        Err(error) => {
            doctor_warn(
                "Git binary",
                format!("could not run git --version: {error}"),
            );
            next_steps
                .push("Repair the git installation so warp can shell out to git.".to_string());
        }
    }
}
