use std::path::{Path, PathBuf};
use std::process::Command;

use super::{doctor_ok, doctor_warn};

pub(super) struct DoctorInstallEntry {
    pub(super) path: PathBuf,
    pub(super) active: bool,
    pub(super) version: Option<String>,
}

pub(super) fn report_doctor_install(next_steps: &mut Vec<String>) {
    let installs = doctor_install_candidates();
    if installs.is_empty() {
        doctor_warn(
            "Install",
            "no warp binary found in PATH or known install dirs",
        );
        #[cfg(windows)]
        next_steps.push(
            "Reinstall with `irm https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.ps1 | iex`."
                .to_string(),
        );
        #[cfg(not(windows))]
        next_steps.push(
            "Reinstall with `curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.sh | sh`."
                .to_string(),
        );
        return;
    }

    let active_index = installs.iter().position(|entry| entry.active);
    let detail = installs
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let marker = if Some(i) == active_index {
                " (active)"
            } else {
                ""
            };
            let version = entry
                .version
                .as_deref()
                .map(|v| format!(" [{v}]"))
                .unwrap_or_default();
            format!("{}{}{}", entry.path.display(), marker, version)
        })
        .collect::<Vec<_>>()
        .join(", ");

    let unique_paths: std::collections::BTreeSet<_> =
        installs.iter().map(|entry| entry.path.clone()).collect();

    if unique_paths.len() > 1 {
        doctor_warn(
            "Install",
            format!("multiple warp binaries detected: {detail}"),
        );
        next_steps.push(
            "Resolve install conflicts: keep one warp binary on PATH, or run the matching uninstaller (`uninstall.sh` or `cargo uninstall git-warp`)."
                .to_string(),
        );
    } else {
        doctor_ok("Install", detail);
    }
}

pub(super) fn doctor_default_install_dir() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("GIT_WARP_DEFAULT_INSTALL_DIR") {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    #[cfg(windows)]
    {
        if let Some(local) = dirs::data_local_dir() {
            return Some(local.join("Programs").join("git-warp").join("bin"));
        }
        dirs::home_dir().map(|h| {
            h.join("AppData")
                .join("Local")
                .join("Programs")
                .join("git-warp")
                .join("bin")
        })
    }
    #[cfg(not(windows))]
    {
        dirs::home_dir().map(|h| h.join(".local").join("bin"))
    }
}

pub(super) fn path_dirs() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default()
}

pub(super) fn same_path(a: &Path, b: &Path) -> bool {
    let canon_a = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let canon_b = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    canon_a == canon_b
}

fn doctor_install_candidates() -> Vec<DoctorInstallEntry> {
    let mut paths = Vec::new();

    if let Some(active) = resolve_warp_on_path() {
        paths.push((active, true));
    }

    for dir in doctor_install_probe_dirs() {
        let candidate = dir.join(crate::release::warp_executable_name());
        if candidate.is_file() {
            paths.push((candidate, false));
        }
    }

    let mut seen = std::collections::HashSet::new();
    let mut entries = Vec::new();
    for (path, active) in paths {
        let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !seen.insert(canonical.clone()) {
            if active
                && let Some(existing) = entries
                    .iter_mut()
                    .find(|e: &&mut DoctorInstallEntry| e.path == path)
            {
                existing.active = true;
            }
            continue;
        }
        entries.push(DoctorInstallEntry {
            version: probe_warp_version(&path),
            path,
            active,
        });
    }
    entries
}

fn doctor_install_probe_dirs() -> Vec<PathBuf> {
    if let Some(override_value) = std::env::var_os("GIT_WARP_DOCTOR_PROBE_DIRS") {
        return std::env::split_paths(&override_value).collect();
    }
    #[allow(unused_mut)]
    let mut dirs = Vec::new();
    #[cfg(windows)]
    {
        if let Some(local) = dirs::data_local_dir() {
            dirs.push(local.join("Programs").join("git-warp").join("bin"));
        }
        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join(".cargo").join("bin"));
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join(".local").join("bin"));
            dirs.push(home.join(".cargo").join("bin"));
        }
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
    }
    dirs
}

pub(super) fn resolve_warp_on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let name = crate::release::warp_executable_name();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(&name);
        if candidate.is_file() && is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn probe_warp_version(path: &Path) -> Option<String> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let line = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    Some(line)
}
