use std::path::{Path, PathBuf};

pub(super) enum DoctorShell {
    Known {
        name: &'static str,
        rc_candidates: Vec<PathBuf>,
    },
    #[cfg_attr(not(windows), allow(dead_code))]
    PowerShell,
    Unknown {
        value: Option<String>,
    },
}

pub(super) fn detect_shell() -> DoctorShell {
    let raw = std::env::var("SHELL").ok().filter(|v| !v.trim().is_empty());
    let basename = raw.as_deref().and_then(|value| {
        value
            .rsplit('/')
            .next()
            .map(str::to_string)
            .filter(|v| !v.is_empty())
    });

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return fallback_shell(raw),
    };

    match basename.as_deref() {
        Some("bash") => DoctorShell::Known {
            name: "bash",
            rc_candidates: bash_rc_candidates(&home),
        },
        Some("zsh") => DoctorShell::Known {
            name: "zsh",
            rc_candidates: vec![home.join(".zshrc")],
        },
        Some("fish") => DoctorShell::Known {
            name: "fish",
            rc_candidates: vec![home.join(".config").join("fish").join("config.fish")],
        },
        _ => fallback_shell(raw),
    }
}

/// Ordered rc-file probe for bash. macOS bash launches as an interactive
/// login shell that reads `~/.bash_profile` before `~/.bashrc`, so the
/// snippet the docs point at may live in any of these files.
fn bash_rc_candidates(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".bashrc"),
        home.join(".bash_profile"),
        home.join(".profile"),
    ]
}

pub(super) fn pick_installed_rc(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|c| shell_rc_has_warp_integration(c))
        .cloned()
}

pub(super) fn pick_display_rc(candidates: &[PathBuf]) -> Option<&Path> {
    candidates
        .iter()
        .find(|c| c.exists())
        .map(PathBuf::as_path)
        .or_else(|| candidates.first().map(PathBuf::as_path))
}

fn fallback_shell(raw: Option<String>) -> DoctorShell {
    #[cfg(windows)]
    {
        if std::env::var_os("PSModulePath").is_some() {
            let _ = raw;
            return DoctorShell::PowerShell;
        }
    }
    DoctorShell::Unknown { value: raw }
}

pub(super) fn shell_rc_has_warp_integration(rc_path: &Path) -> bool {
    std::fs::read_to_string(rc_path)
        .map(|content| content.contains("warp_cd") || content.contains("warp __complete"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_rc_candidates_probe_bashrc_bash_profile_and_profile() {
        let home = PathBuf::from("/users/alice");
        assert_eq!(
            bash_rc_candidates(&home),
            vec![
                PathBuf::from("/users/alice/.bashrc"),
                PathBuf::from("/users/alice/.bash_profile"),
                PathBuf::from("/users/alice/.profile"),
            ],
        );
    }

    #[test]
    fn pick_installed_rc_finds_bash_profile_when_only_it_has_snippet() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path();
        let bash_profile = home.join(".bash_profile");
        std::fs::write(
            &bash_profile,
            "# git-warp integration\nwarp_cd() { eval \"$(warp --terminal echo \"$@\")\"; }\n",
        )
        .unwrap();

        let candidates = bash_rc_candidates(home);
        let picked = pick_installed_rc(&candidates).expect("bash_profile picked");
        assert_eq!(picked, bash_profile);
    }

    #[test]
    fn pick_installed_rc_prefers_bashrc_when_both_contain_snippet() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path();
        std::fs::write(home.join(".bashrc"), "warp_cd() { :; }\n").unwrap();
        std::fs::write(home.join(".bash_profile"), "warp_cd() { :; }\n").unwrap();

        let candidates = bash_rc_candidates(home);
        let picked = pick_installed_rc(&candidates).expect("some rc picked");
        assert_eq!(picked, home.join(".bashrc"));
    }

    #[test]
    fn pick_display_rc_prefers_existing_candidate() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path();
        std::fs::write(home.join(".bash_profile"), "# empty\n").unwrap();

        let candidates = bash_rc_candidates(home);
        let picked = pick_display_rc(&candidates).expect("some rc picked");
        assert_eq!(picked, home.join(".bash_profile"));
    }

    #[test]
    fn pick_display_rc_falls_back_to_first_when_none_exist() {
        let home = PathBuf::from("/nonexistent/home");
        let candidates = bash_rc_candidates(&home);
        let picked = pick_display_rc(&candidates).expect("first candidate");
        assert_eq!(picked, PathBuf::from("/nonexistent/home/.bashrc"));
    }
}
