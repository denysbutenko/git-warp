use anyhow::Result;
use std::path::Path;
use std::process::Command;

use crate::cli::Cli;
use crate::config::TerminalConfig;
use crate::terminal::TerminalMode;

use super::report::SwitchOutcomeReport;

pub(super) fn resolve_terminal_mode(cli: &Cli, config_mode: &str) -> Result<TerminalMode> {
    if let Some(mode) = cli.terminal {
        return Ok(mode);
    }
    TerminalMode::from_str(config_mode).ok_or_else(|| {
        anyhow::anyhow!(
            "Invalid terminal_mode '{}' in config. Supported modes: {}",
            config_mode,
            TerminalMode::SUPPORTED.join(", "),
        )
    })
}

pub(super) fn record_branch_checkout(
    report: &mut SwitchOutcomeReport,
    worktree_path: &Path,
    branch: &str,
    checkout_warning: Option<String>,
) {
    match current_branch_at_path(worktree_path) {
        Ok(current_branch) if current_branch == branch && checkout_warning.is_none() => {
            report.done("Branch checkout", branch);
        }
        Ok(current_branch) if current_branch == branch => {
            report.warned(
                "Branch checkout",
                format!(
                    "checkout reported warning for {}: {}",
                    branch,
                    checkout_warning.unwrap_or_default()
                ),
            );
        }
        Ok(current_branch) => {
            let found = if current_branch.is_empty() {
                "detached HEAD".to_string()
            } else {
                current_branch
            };
            let detail = match checkout_warning {
                Some(warning) if !warning.is_empty() => {
                    format!("expected {branch}, found {found}; checkout failed: {warning}")
                }
                _ => format!(
                    "expected {branch}, found {found}. Use a different --path or run `warp ls` to inspect worktrees."
                ),
            };
            report.warned("Branch checkout", detail);
        }
        Err(error) => {
            let detail = match checkout_warning {
                Some(warning) if !warning.is_empty() => {
                    format!("could not verify {branch}: {error}; checkout failed: {warning}")
                }
                _ => format!("could not verify {branch}: {error}"),
            };
            report.warned("Branch checkout", detail);
        }
    }
}

pub(super) fn current_branch_at_path(worktree_path: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(worktree_path)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to verify worktree branch: {}", e))?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "Failed to verify worktree branch: {}",
            error.trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(super) fn record_terminal_handoff(
    report: &mut SwitchOutcomeReport,
    worktree_path: &Path,
    terminal_mode: TerminalMode,
    terminal_app: &str,
    terminal_config: &TerminalConfig,
) {
    use crate::git::GitRepository;
    use crate::terminal::{TerminalLaunchOptions, TerminalManager};

    let branch = current_branch_at_path(worktree_path).unwrap_or_else(|_| "unknown".to_string());
    let repo = GitRepository::find()
        .map(|r| r.repo_name())
        .unwrap_or_else(|_| "unknown".to_string());

    let terminal_manager = TerminalManager;
    let launch_options = TerminalLaunchOptions {
        auto_activate: terminal_config.auto_activate,
        init_commands: terminal_config.init_commands.clone(),
        branch: Some(branch),
        repo: Some(repo),
    };

    match terminal_manager.switch_to_worktree_with_options(
        worktree_path,
        terminal_mode,
        None,
        Some(terminal_app),
        &launch_options,
    ) {
        Ok(()) => {
            report.done(
                "Terminal handoff",
                terminal_handoff_success_detail(terminal_mode),
            );
        }
        Err(e) => {
            log::warn!("Terminal switching failed: {}", e);
            report.warned(
                "Terminal handoff",
                format!(
                    "failed: {e}. Retry with `--terminal echo` to print manual commands instead."
                ),
            );
        }
    }
}

fn terminal_handoff_success_detail(terminal_mode: TerminalMode) -> &'static str {
    match terminal_mode {
        TerminalMode::Tab => "opened tab",
        TerminalMode::Window => "opened window",
        TerminalMode::InPlace => "printed cd command",
        TerminalMode::Echo => "printed manual commands",
        TerminalMode::Current => "started current-terminal shell",
    }
}
