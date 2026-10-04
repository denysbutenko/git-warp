use super::HooksManager;
use super::types::HookRuntime;
use crate::error::Result;
use chrono::Local;
use serde_json::json;
use std::fs;
use std::path::Path;

impl HooksManager {
    /// Write the per-runtime live status file used by `warp agents` and the
    /// TUI dashboard. Called from the hidden `warp __hook-status` subcommand
    /// installed into Claude/Codex hook configs (#189).
    pub fn write_runtime_status(runtime_arg: &str, status: &str) -> Result<()> {
        let repo_root = crate::git::GitRepository::find()
            .map(|repo| repo.root_path().to_path_buf())
            .or_else(|_| std::env::current_dir())?;

        Self::write_runtime_status_with_root(&repo_root, runtime_arg, status)
    }

    /// Root-explicit variant of [`Self::write_runtime_status`]. Keeps tests
    /// off the process-wide CWD (#238) so parallel lib-crate unit tests do
    /// not race on `std::env::set_current_dir`.
    pub fn write_runtime_status_with_root(
        repo_root: &Path,
        runtime_arg: &str,
        status: &str,
    ) -> Result<()> {
        let runtime = HookRuntime::parse_single(runtime_arg)?;

        let dir = repo_root.join(runtime.status_root_dir()).join("git-warp");
        fs::create_dir_all(&dir)?;
        let path = dir.join("status");

        let payload = json!({
            "status": status,
            "last_activity": Local::now().to_rfc3339(),
        });
        let serialized = serde_json::to_string(&payload)?;

        crate::fs_atomic::write_atomic(&path, serialized.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::HooksManager;
    use serde_json::Value;
    use std::fs;

    #[test]
    fn test_write_runtime_status_writes_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        // canonicalize so we match whatever `/private/var/...`-style path
        // the platform actually resolves the tempdir to.
        let repo_root = std::fs::canonicalize(temp_dir.path()).unwrap();

        HooksManager::write_runtime_status_with_root(&repo_root, "claude", "waiting").unwrap();

        let status_path = repo_root.join(".claude").join("git-warp").join("status");
        let body = fs::read_to_string(&status_path).unwrap();
        let parsed: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["status"].as_str(), Some("waiting"));
        let last_activity = parsed["last_activity"].as_str().unwrap();
        assert!(
            chrono::DateTime::parse_from_rfc3339(last_activity).is_ok(),
            "last_activity {last_activity:?} is not RFC3339"
        );
    }

    #[test]
    fn test_write_runtime_status_rejects_all() {
        let err = HooksManager::write_runtime_status("all", "waiting").unwrap_err();
        assert!(err.to_string().contains("claude or codex"));
    }
}
