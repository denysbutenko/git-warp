use super::install_probe::{
    doctor_default_install_dir, path_dirs, resolve_warp_on_path, same_path,
};
use super::profile_paths::powershell_profile_paths;
use super::shells::{
    DoctorShell, detect_shell, pick_display_rc, pick_installed_rc, shell_rc_has_warp_integration,
};
use super::{doctor_ok, doctor_warn};

pub(super) fn report_doctor_shell_integration(next_steps: &mut Vec<String>) {
    let active = resolve_warp_on_path();
    let default_dir = doctor_default_install_dir();
    let path_dirs = path_dirs();
    let install_on_path = default_dir
        .as_ref()
        .map(|dir| path_dirs.iter().any(|d| same_path(d, dir)))
        .unwrap_or(false);

    match (&active, &default_dir) {
        (Some(path), _) => doctor_ok("Shell PATH", format!("active warp at {}", path.display())),
        (None, Some(dir)) => {
            doctor_warn(
                "Shell PATH",
                format!(
                    "no warp on PATH (expected {})",
                    dir.join(crate::release::warp_executable_name()).display()
                ),
            );
            next_steps.push(format!(
                "Add `{}` to PATH (e.g. `export PATH=\"{}:$PATH\"`).",
                dir.display(),
                dir.display()
            ));
        }
        (None, None) => {
            doctor_warn("Shell PATH", "no warp on PATH and HOME is not set");
        }
    }

    if let Some(dir) = &default_dir {
        if !install_on_path {
            doctor_warn(
                "Default install path",
                format!("{} is not in PATH", dir.display()),
            );
            next_steps.push(format!(
                "Add `{}` to PATH (e.g. `export PATH=\"{}:$PATH\"`) so installed warp is found.",
                dir.display(),
                dir.display()
            ));
        } else if let Some(active_path) = &active {
            let installed = dir.join(crate::release::warp_executable_name());
            if installed.is_file() && !same_path(active_path, &installed) {
                doctor_warn(
                    "Default install path",
                    format!(
                        "{} is on PATH but a different warp resolves first at {}",
                        dir.display(),
                        active_path.display()
                    ),
                );
                next_steps.push(format!(
                    "Reorder PATH to put `{}` before `{}`.",
                    dir.display(),
                    active_path
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| active_path.display().to_string())
                ));
            }
        }
    }

    let shell = detect_shell();
    match &shell {
        DoctorShell::Known {
            name,
            rc_candidates,
        } => {
            if let Some(rc_path) = pick_installed_rc(rc_candidates) {
                let rc_label = rc_path.display().to_string();
                if active.is_none() {
                    doctor_warn(
                        "Shell integration",
                        format!("warp_cd helper detected in {rc_label} but 'warp' is not on PATH"),
                    );
                    next_steps.push(format!(
                        "Remove Git-Warp snippets from {rc_label} or fix your PATH."
                    ));
                } else {
                    doctor_ok(
                        "Shell integration",
                        format!("warp_cd helper detected in {rc_label}"),
                    );
                }
            } else {
                let rc_label = pick_display_rc(rc_candidates)
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                doctor_warn(
                    "Shell integration",
                    format!("warp_cd helper not found in {rc_label}"),
                );
                next_steps.push(format!(
                    "Run `warp shell-config {name}` and append the output to {rc_label} to enable `warp_cd` and completions."
                ));
            }
        }
        DoctorShell::PowerShell => {
            let profile_paths = powershell_profile_paths();
            let integrated = profile_paths
                .iter()
                .any(|path| shell_rc_has_warp_integration(path));
            if integrated {
                doctor_ok(
                    "Shell integration",
                    "warp_cd helper detected in a PowerShell profile",
                );
            } else {
                doctor_warn(
                    "Shell integration",
                    "PowerShell warp_cd helper is not installed",
                );
                next_steps.push(
                    "Run `warp shell-config powershell` and append the output to $PROFILE.CurrentUserAllHosts to enable `warp_cd` and completions. OneDrive-redirected profile locations (under `OneDrive\\Documents\\PowerShell`) are also checked.".to_string(),
                );
            }
            if let Some(dir) = &default_dir {
                next_steps.push(format!(
                    "Add `{}` to $env:PATH (e.g. `$env:Path = \"{};$env:Path\"`) so installed {} is found.",
                    dir.display(),
                    dir.display(),
                    crate::release::warp_executable_name(),
                ));
            } else {
                next_steps.push(
                    "Install Git-Warp via `irm https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.ps1 | iex` and add the install directory to $env:PATH."
                        .to_string(),
                );
            }
        }
        DoctorShell::Unknown { value } => {
            let detail = match value {
                Some(v) => {
                    format!("unsupported shell `{v}`; supported: bash, zsh, fish, powershell")
                }
                None => "SHELL is not set".to_string(),
            };
            doctor_warn("Shell integration", detail);
            next_steps.push(
                "Set SHELL to bash, zsh, or fish (or use PowerShell), then run `warp shell-config <shell>` for setup snippets.".to_string(),
            );
        }
    }
}
