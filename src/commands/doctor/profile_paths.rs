use std::path::{Path, PathBuf};

use super::windows::documents_known_folder;

pub(super) fn powershell_profile_paths() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let docs = powershell_documents_dirs(&home);
    powershell_profile_paths_with_documents(&docs)
}

/// Produce the candidate `$PROFILE` file paths under each Documents dir.
/// Order matches PowerShell's precedence: `profile.ps1` (AllHosts) before
/// `Microsoft.PowerShell_profile.ps1` (CurrentHost), for both the modern
/// `PowerShell` host and the legacy `WindowsPowerShell` host.
fn powershell_profile_paths_with_documents(documents: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths = Vec::with_capacity(documents.len() * 4);
    for docs in documents {
        for host in ["PowerShell", "WindowsPowerShell"] {
            paths.push(docs.join(host).join("profile.ps1"));
            paths.push(docs.join(host).join("Microsoft.PowerShell_profile.ps1"));
        }
    }
    paths
}

/// Resolve every Documents directory PowerShell could be using: the
/// Windows Known Folder (which follows OneDrive KFM redirection), the
/// `OneDrive` / `OneDriveCommercial` environment fallbacks OneDrive
/// exports on managed installs, `home/OneDrive/Documents`, and the plain
/// `home/Documents` legacy path. Order is preserved so higher-precedence
/// hits are probed first; duplicates are removed.
fn powershell_documents_dirs(home: &Path) -> Vec<PathBuf> {
    let known = documents_known_folder();
    let onedrive = std::env::var_os("OneDrive").map(PathBuf::from);
    let onedrive_commercial = std::env::var_os("OneDriveCommercial").map(PathBuf::from);
    powershell_documents_dirs_with_env(
        home,
        known.as_deref(),
        onedrive.as_deref(),
        onedrive_commercial.as_deref(),
    )
}

fn powershell_documents_dirs_with_env(
    home: &Path,
    known: Option<&Path>,
    onedrive: Option<&Path>,
    onedrive_commercial: Option<&Path>,
) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut push = |candidate: PathBuf| {
        if candidate.as_os_str().is_empty() {
            return;
        }
        if !dirs.iter().any(|existing| existing == &candidate) {
            dirs.push(candidate);
        }
    };
    if let Some(known) = known {
        push(known.to_path_buf());
    }
    for base in [onedrive, onedrive_commercial].into_iter().flatten() {
        push(base.join("Documents"));
    }
    push(home.join("OneDrive").join("Documents"));
    push(home.join("Documents"));
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powershell_profile_paths_expands_each_documents_dir() {
        let docs = vec![
            PathBuf::from("/users/alice/OneDrive/Documents"),
            PathBuf::from("/users/alice/Documents"),
        ];
        assert_eq!(
            powershell_profile_paths_with_documents(&docs),
            vec![
                PathBuf::from("/users/alice/OneDrive/Documents/PowerShell/profile.ps1"),
                PathBuf::from(
                    "/users/alice/OneDrive/Documents/PowerShell/Microsoft.PowerShell_profile.ps1"
                ),
                PathBuf::from("/users/alice/OneDrive/Documents/WindowsPowerShell/profile.ps1"),
                PathBuf::from(
                    "/users/alice/OneDrive/Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1"
                ),
                PathBuf::from("/users/alice/Documents/PowerShell/profile.ps1"),
                PathBuf::from("/users/alice/Documents/PowerShell/Microsoft.PowerShell_profile.ps1"),
                PathBuf::from("/users/alice/Documents/WindowsPowerShell/profile.ps1"),
                PathBuf::from(
                    "/users/alice/Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1"
                ),
            ],
        );
    }

    #[test]
    fn powershell_profile_paths_empty_when_no_documents_dirs() {
        assert!(powershell_profile_paths_with_documents(&[]).is_empty());
    }

    #[test]
    fn documents_dirs_prefer_known_folder_and_include_plain_fallback() {
        // Simulates OneDrive KFM: the Known Folder resolves to the OneDrive
        // Documents path, but the legacy `~/Documents` probe is still added
        // last so redirected and non-redirected setups both keep working.
        let home = PathBuf::from("/users/alice");
        let known = PathBuf::from("/users/alice/OneDrive/Documents");
        let dirs = powershell_documents_dirs_with_env(&home, Some(&known), None, None);
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/users/alice/OneDrive/Documents"),
                PathBuf::from("/users/alice/Documents"),
            ],
        );
    }

    #[test]
    fn documents_dirs_honor_onedrive_env_when_known_folder_missing() {
        let home = PathBuf::from("/users/alice");
        let onedrive = PathBuf::from("/users/alice/OneDrive - Contoso");
        let dirs = powershell_documents_dirs_with_env(&home, None, Some(&onedrive), None);
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/users/alice/OneDrive - Contoso/Documents"),
                PathBuf::from("/users/alice/OneDrive/Documents"),
                PathBuf::from("/users/alice/Documents"),
            ],
        );
    }

    #[test]
    fn documents_dirs_dedupe_overlapping_sources() {
        // Known Folder, `$env:OneDrive\Documents`, and `home/OneDrive/Documents`
        // can all resolve to the same location; probe it only once.
        let home = PathBuf::from("/users/alice");
        let onedrive = PathBuf::from("/users/alice/OneDrive");
        let known = PathBuf::from("/users/alice/OneDrive/Documents");
        let dirs = powershell_documents_dirs_with_env(
            &home,
            Some(&known),
            Some(&onedrive),
            Some(&onedrive),
        );
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/users/alice/OneDrive/Documents"),
                PathBuf::from("/users/alice/Documents"),
            ],
        );
    }

    #[test]
    fn documents_dirs_fallback_when_no_hints() {
        let home = PathBuf::from("/users/alice");
        let dirs = powershell_documents_dirs_with_env(&home, None, None, None);
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/users/alice/OneDrive/Documents"),
                PathBuf::from("/users/alice/Documents"),
            ],
        );
    }
}
