use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::Cli;

enum DoctorShell {
    Known {
        name: &'static str,
        rc_path: PathBuf,
    },
    #[cfg_attr(not(windows), allow(dead_code))]
    PowerShell,
    Unknown {
        value: Option<String>,
    },
}

struct DoctorInstallEntry {
    path: PathBuf,
    active: bool,
    version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DoctorHookSeverity {
    Healthy,
    Partial,
    Missing,
}

struct DoctorHooksSummary {
    severity: DoctorHookSeverity,
    detail: String,
    next_steps: Vec<String>,
}

pub fn run(_cli: &Cli) -> Result<()> {
    use crate::config::ConfigManager;
    use crate::cow;
    use crate::git::GitRepository;

    let config_manager = ConfigManager::new()?;
    let config = config_manager.get();
    let repo = GitRepository::find().ok();
    let mut next_steps = Vec::new();

    println!("🩺 Git-Warp Doctor");
    println!("==================");
    println!();
    println!("Checks:");

    if config_manager.config_exists() {
        doctor_ok(
            "Config file",
            format!("found at {}", config_manager.config_path().display()),
        );
    } else {
        doctor_warn(
            "Config file",
            format!("missing at {}", config_manager.config_path().display()),
        );
        next_steps.push("Run `warp config --edit` to create and review your config.".to_string());
    }

    match &repo {
        Some(repo) => {
            doctor_ok("Git repository", repo.root_path().display().to_string());
        }
        None => {
            doctor_warn("Git repository", "not detected from current directory");
            next_steps.push(
                "Run this command inside a Git repository before creating worktrees.".to_string(),
            );
        }
    }

    report_doctor_git_binary(&mut next_steps);

    let worktree_base = if let Some(repo) = &repo {
        let sample_worktree =
            repo.get_worktree_path_with_base("doctor-check", config.worktrees_path.as_deref());
        let base = sample_worktree
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| sample_worktree.clone());
        doctor_info("Worktree base path", base.display().to_string());
        base
    } else {
        config
            .worktrees_path
            .clone()
            .unwrap_or_else(|| PathBuf::from(".worktrees"))
    };

    let cow_check_path = nearest_existing_parent(&worktree_base);
    let cow_supported = cow::is_cow_supported(&cow_check_path);
    match &cow_supported {
        Ok(true) => doctor_ok(
            "Copy-on-Write",
            format!(
                "available on filesystem containing {}",
                worktree_base.display()
            ),
        ),
        Ok(false) => doctor_info(
            "Copy-on-Write",
            "not available on this filesystem; Git-Warp will use git worktree add",
        ),
        Err(error) => doctor_warn("Copy-on-Write", format!("could not check support: {error}")),
    }

    doctor_info(
        "Terminal",
        format!("mode {}, app {}", config.terminal_mode, config.terminal.app),
    );

    if repo.is_none() || !matches!(cow_supported, Ok(true)) {
        next_steps.push(
            "Run `warp switch --no-cow <branch>` to skip CoW checks for a switch.".to_string(),
        );
    }

    let hooks_summary = doctor_hooks_summary();
    match hooks_summary.severity {
        DoctorHookSeverity::Healthy => {
            doctor_ok("Agent hooks", hooks_summary.detail);
        }
        DoctorHookSeverity::Partial => {
            doctor_warn("Agent hooks", hooks_summary.detail);
            for step in &hooks_summary.next_steps {
                next_steps.push(step.clone());
            }
        }
        DoctorHookSeverity::Missing => {
            doctor_warn("Agent hooks", hooks_summary.detail);
            for step in &hooks_summary.next_steps {
                next_steps.push(step.clone());
            }
        }
    }
    let hooks_installed = matches!(hooks_summary.severity, DoctorHookSeverity::Healthy);

    report_doctor_install(&mut next_steps);
    report_doctor_shell_integration(&mut next_steps);

    if repo.is_some() && config_manager.config_exists() && hooks_installed {
        next_steps.push("Run `warp switch <branch>` to create or open a worktree.".to_string());
    }

    println!();
    println!("Next steps:");
    if next_steps.is_empty() {
        println!("  - No immediate setup steps found.");
    } else {
        for step in next_steps {
            println!("  - {step}");
        }
    }

    Ok(())
}

fn doctor_ok(label: &str, detail: impl AsRef<str>) {
    println!("  ✅ {label}: {}", detail.as_ref());
}

fn doctor_warn(label: &str, detail: impl AsRef<str>) {
    println!("  ⚠️  {label}: {}", detail.as_ref());
}

fn doctor_info(label: &str, detail: impl AsRef<str>) {
    println!("  ℹ️  {label}: {}", detail.as_ref());
}

fn nearest_existing_parent(path: &Path) -> PathBuf {
    let mut candidate = path.to_path_buf();
    while !candidate.exists() {
        if !candidate.pop() {
            return PathBuf::from(".");
        }
    }
    candidate
}

fn report_doctor_git_binary(next_steps: &mut Vec<String>) {
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

fn report_doctor_install(next_steps: &mut Vec<String>) {
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

fn report_doctor_shell_integration(next_steps: &mut Vec<String>) {
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
        DoctorShell::Known { name, rc_path } => {
            let rc_label = rc_path.display().to_string();
            let configured = shell_rc_has_warp_integration(rc_path);
            if configured {
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

fn doctor_default_install_dir() -> Option<PathBuf> {
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

fn path_dirs() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default()
}

fn same_path(a: &Path, b: &Path) -> bool {
    let canon_a = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let canon_b = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    canon_a == canon_b
}

fn detect_shell() -> DoctorShell {
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
            rc_path: home.join(".bashrc"),
        },
        Some("zsh") => DoctorShell::Known {
            name: "zsh",
            rc_path: home.join(".zshrc"),
        },
        Some("fish") => DoctorShell::Known {
            name: "fish",
            rc_path: home.join(".config").join("fish").join("config.fish"),
        },
        _ => fallback_shell(raw),
    }
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

fn shell_rc_has_warp_integration(rc_path: &Path) -> bool {
    std::fs::read_to_string(rc_path)
        .map(|content| content.contains("warp_cd") || content.contains("warp __complete"))
        .unwrap_or(false)
}

fn powershell_profile_paths() -> Vec<PathBuf> {
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

#[cfg(windows)]
fn documents_known_folder() -> Option<PathBuf> {
    use std::ffi::{OsString, c_void};
    use std::os::windows::ffi::OsStringExt;
    use std::ptr;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_Documents, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
    };

    let mut wpath: windows_sys::core::PWSTR = ptr::null_mut();
    // SAFETY: FFI call into the Shell known-folder API. `wpath` is an out
    // parameter the callee allocates via `CoTaskMemAlloc`; we free it via
    // `CoTaskMemFree` on every exit path (including error).
    let hr = unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_Documents,
            KF_FLAG_DEFAULT as u32,
            ptr::null_mut(),
            &mut wpath,
        )
    };
    if hr < 0 || wpath.is_null() {
        if !wpath.is_null() {
            unsafe { CoTaskMemFree(wpath.cast::<c_void>()) };
        }
        return None;
    }
    let mut len = 0isize;
    // SAFETY: SHGetKnownFolderPath returned a NUL-terminated UTF-16 buffer.
    while unsafe { *wpath.offset(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` bounds the buffer up to (but not including) the NUL.
    let slice = unsafe { std::slice::from_raw_parts(wpath, len as usize) };
    let os = OsString::from_wide(slice);
    unsafe { CoTaskMemFree(wpath.cast::<c_void>()) };
    let path = PathBuf::from(os);
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path)
    }
}

#[cfg(not(windows))]
fn documents_known_folder() -> Option<PathBuf> {
    None
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

fn resolve_warp_on_path() -> Option<PathBuf> {
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

fn doctor_hooks_summary() -> DoctorHooksSummary {
    use crate::hooks::{HookState, HooksManager};

    let diagnoses = match HooksManager::diagnose("all") {
        Ok(d) => d,
        Err(_) => {
            return DoctorHooksSummary {
                severity: DoctorHookSeverity::Missing,
                detail: "unable to inspect hook configuration".to_string(),
                next_steps: vec![
                    "Run `warp hooks-status --runtime all` to investigate.".to_string(),
                ],
            };
        }
    };

    let mut healthy: Vec<String> = Vec::new();
    let mut partial: Vec<String> = Vec::new();
    let mut conflicting: Vec<String> = Vec::new();
    let mut next_steps: Vec<String> = Vec::new();

    for d in &diagnoses {
        let scope_label = match d.scope {
            crate::hooks::HookScope::User => "user",
            crate::hooks::HookScope::Project => "project",
        };
        let runtime_label = match d.runtime {
            crate::hooks::HookRuntime::Claude => "claude",
            crate::hooks::HookRuntime::Codex => "codex",
        };
        let combined = format!("{scope_label} {runtime_label}");

        match d.state {
            HookState::Complete => healthy.push(combined),
            HookState::Partial => {
                partial.push(combined);
                next_steps.push(format!(
                    "Run `{}` to repair partial {} {} hooks.",
                    d.install_command(),
                    scope_label,
                    runtime_label
                ));
            }
            HookState::Conflicting => {
                conflicting.push(combined);
                next_steps.push(format!(
                    "Run `{}` to deduplicate {} {} hooks.",
                    d.install_command(),
                    scope_label,
                    runtime_label
                ));
            }
            HookState::Missing | HookState::NotConfigured => {}
        }
    }

    if healthy.is_empty() && partial.is_empty() && conflicting.is_empty() {
        return DoctorHooksSummary {
            severity: DoctorHookSeverity::Missing,
            detail: "no user or project git-warp hooks found".to_string(),
            next_steps: vec![
                "Run `warp hooks-install --level user --runtime all` to enable live agent monitoring.".to_string(),
            ],
        };
    }

    if !partial.is_empty() || !conflicting.is_empty() {
        let mut detail_parts = Vec::new();
        if !healthy.is_empty() {
            detail_parts.push(format!("complete: {}", healthy.join(", ")));
        }
        if !partial.is_empty() {
            detail_parts.push(format!("partial: {}", partial.join(", ")));
        }
        if !conflicting.is_empty() {
            detail_parts.push(format!("conflicting: {}", conflicting.join(", ")));
        }
        return DoctorHooksSummary {
            severity: DoctorHookSeverity::Partial,
            detail: detail_parts.join("; "),
            next_steps,
        };
    }

    DoctorHooksSummary {
        severity: DoctorHookSeverity::Healthy,
        detail: format!("complete: {}", healthy.join(", ")),
        next_steps,
    }
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
