use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::cli::Cli;

mod git_binary;
mod hooks_diag;
mod install_probe;
mod profile_paths;
mod shell_integration;
mod shells;
mod windows;

use git_binary::report_doctor_git_binary;
use hooks_diag::{DoctorHookSeverity, doctor_hooks_summary};
use install_probe::report_doctor_install;
use shell_integration::report_doctor_shell_integration;

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

pub(super) fn doctor_ok(label: &str, detail: impl AsRef<str>) {
    println!("  ✅ {label}: {}", detail.as_ref());
}

pub(super) fn doctor_warn(label: &str, detail: impl AsRef<str>) {
    println!("  ⚠️  {label}: {}", detail.as_ref());
}

pub(super) fn doctor_info(label: &str, detail: impl AsRef<str>) {
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
