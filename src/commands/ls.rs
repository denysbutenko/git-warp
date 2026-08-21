use anyhow::Result;
use log::info;

use crate::cli::Cli;

struct LsRow {
    worktree: crate::git::WorktreeInfo,
    is_dirty: bool,
    is_busy: bool,
}

/// Decide whether `warp ls` should open the interactive switcher instead of
/// printing the table. `--interactive` always wins; on a plain TTY without
/// flags we default to the switcher, but `--dry-run` and `--debug` both ask
/// for the table path explicitly (regression for #269).
fn should_open_ls_switcher(interactive: bool, is_tty: bool, dry_run: bool, debug: bool) -> bool {
    interactive || (is_tty && !dry_run && !debug)
}

pub fn run(cli: &Cli, debug: bool, interactive: bool) -> Result<()> {
    use crate::git::GitRepository;
    use crate::process::ProcessManager;
    use std::io::IsTerminal;

    use super::switch;
    use super::util::not_in_git_repo_error;

    info!("Listing worktrees");

    if should_open_ls_switcher(
        interactive,
        std::io::stdout().is_terminal(),
        cli.dry_run,
        debug,
    ) {
        return switch::run_default(cli);
    }

    let git_repo = GitRepository::find().map_err(|_| not_in_git_repo_error())?;

    if cli.dry_run {
        println!("Would list all worktrees");
        return Ok(());
    }

    let worktrees = git_repo.list_worktrees()?;
    let mut process_manager = ProcessManager::new();

    if worktrees.is_empty() {
        println!("📭 No worktrees found");
        return Ok(());
    }

    process_manager.refresh();
    let mut rows: Vec<LsRow> = worktrees
        .into_iter()
        .map(|worktree| {
            let is_dirty = git_repo
                .has_uncommitted_changes(&worktree.path)
                .unwrap_or(false);
            let is_busy = !worktree.is_current
                && process_manager
                    .has_processes_in_directory(&worktree.path)
                    .unwrap_or(false);
            LsRow {
                worktree,
                is_dirty,
                is_busy,
            }
        })
        .collect();

    rows.sort_by(|a, b| {
        worktree_sort_priority(&a.worktree, a.is_dirty, a.is_busy)
            .cmp(&worktree_sort_priority(&b.worktree, b.is_dirty, b.is_busy))
            .then_with(|| a.worktree.branch.cmp(&b.worktree.branch))
    });

    println!("📁 Git Worktrees:");
    println!();

    let current_summary = rows
        .iter()
        .find(|r| r.worktree.is_current)
        .map(|r| ls_branch_display(&r.worktree));
    let primary_summary = rows
        .iter()
        .find(|r| r.worktree.is_primary && !r.worktree.is_current)
        .map(|r| ls_branch_display(&r.worktree));

    if let Some(branch) = &current_summary {
        println!("📍 Current: {}", branch);
    }
    if let Some(branch) = &primary_summary {
        println!("🏠 Primary: {}", branch);
    }
    if current_summary.is_some() || primary_summary.is_some() {
        println!();
    }

    for (i, row) in rows.iter().enumerate() {
        let LsRow {
            worktree,
            is_dirty,
            is_busy,
        } = row;
        let status_icon = worktree_row_icon(worktree, *is_dirty, *is_busy);
        let branch_display = ls_branch_display(worktree);
        let labels = worktree_status_labels(worktree, *is_dirty, *is_busy);
        let label_display = format_status_labels(&labels);

        println!(
            "{}  {}{} {}",
            status_icon,
            branch_display,
            label_display,
            worktree.path.display()
        );

        if debug {
            println!("     HEAD: {}", worktree.head);
            println!("     Primary: {}", worktree.is_primary);
            println!("     Current: {}", worktree.is_current);
            println!("     Detached: {}", worktree.is_detached);
            println!("     Dirty: {}", is_dirty);
            println!("     Busy: {}", is_busy);
            if i < rows.len() - 1 {
                println!();
            }
        }
    }

    println!();
    println!("📊 Total: {} worktrees", rows.len());

    Ok(())
}

fn ls_branch_display(worktree: &crate::git::WorktreeInfo) -> String {
    if worktree.is_detached {
        let short_head: String = worktree.head.chars().take(8).collect();
        format!("(detached: {})", short_head)
    } else {
        worktree.branch.clone()
    }
}

fn worktree_row_icon(
    worktree: &crate::git::WorktreeInfo,
    is_dirty: bool,
    is_busy: bool,
) -> &'static str {
    if worktree.is_current {
        "👉"
    } else if worktree.is_primary {
        "🏠"
    } else if is_dirty {
        "⚠️ "
    } else if is_busy {
        "⏳"
    } else if worktree.is_detached {
        "🔍"
    } else {
        "🌿"
    }
}

fn worktree_sort_priority(
    worktree: &crate::git::WorktreeInfo,
    is_dirty: bool,
    is_busy: bool,
) -> u8 {
    if worktree.is_current {
        0
    } else if worktree.is_primary {
        1
    } else if is_dirty {
        2
    } else if is_busy {
        3
    } else if worktree.is_detached {
        4
    } else {
        5
    }
}

fn worktree_status_labels(
    worktree: &crate::git::WorktreeInfo,
    is_dirty: bool,
    is_busy: bool,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if worktree.is_primary {
        labels.push("primary");
    }
    if worktree.is_current {
        labels.push("current");
    }
    if is_dirty {
        labels.push("dirty");
    }
    if worktree.is_detached {
        labels.push("detached");
    }
    if is_busy {
        labels.push("busy");
    }
    labels
}

fn format_status_labels(labels: &[&str]) -> String {
    if labels.is_empty() {
        String::new()
    } else {
        format!(" [{}]", labels.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ls_switcher_gate_non_tty_prints_table() {
        assert!(!should_open_ls_switcher(false, false, false, false));
        assert!(!should_open_ls_switcher(false, false, true, false));
        assert!(!should_open_ls_switcher(false, false, false, true));
    }

    #[test]
    fn ls_switcher_gate_tty_opens_switcher_by_default() {
        assert!(should_open_ls_switcher(false, true, false, false));
    }

    #[test]
    fn ls_switcher_gate_debug_on_tty_prints_table() {
        // Regression for #269: `warp ls --debug` on a TTY must fall through to
        // the debug-row-printing path instead of silently opening the switcher.
        assert!(!should_open_ls_switcher(false, true, false, true));
    }

    #[test]
    fn ls_switcher_gate_dry_run_on_tty_prints_table() {
        assert!(!should_open_ls_switcher(false, true, true, false));
    }

    #[test]
    fn ls_switcher_gate_interactive_wins_over_debug() {
        // Explicit `--interactive` overrides `--debug` even on a TTY.
        assert!(should_open_ls_switcher(true, true, false, true));
        assert!(should_open_ls_switcher(true, false, false, true));
    }
}
