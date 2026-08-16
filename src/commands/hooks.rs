use anyhow::Result;
use log::info;

use crate::cli::Cli;

pub fn install(
    cli: &Cli,
    level: Option<crate::hooks::HookInstallLevel>,
    runtime: &str,
) -> Result<()> {
    use crate::hooks::{HookInstallLevel, HooksManager};

    let resolved = level.unwrap_or(HookInstallLevel::Console);
    info!(
        "Installing hooks at level: {}, runtime: {}",
        resolved, runtime
    );
    if cli.dry_run {
        println!(
            "Would install Git-Warp hooks at level: {}, runtime: {}",
            resolved, runtime
        );
        return Ok(());
    }

    HooksManager::install_hooks(resolved, runtime)
}

pub fn remove(
    cli: &Cli,
    level: Option<crate::hooks::HookRemoveLevel>,
    runtime: &str,
) -> Result<()> {
    use crate::hooks::{HookRemoveLevel, HooksManager};

    let resolved = level.unwrap_or(HookRemoveLevel::User);
    info!(
        "Removing hooks at level: {}, runtime: {}",
        resolved, runtime
    );
    if cli.dry_run {
        println!(
            "Would remove Git-Warp hooks at level: {}, runtime: {}",
            resolved, runtime
        );
        return Ok(());
    }

    HooksManager::remove_hooks(resolved, runtime)
}

pub fn status(_cli: &Cli, runtime: &str) -> Result<()> {
    use crate::hooks::HooksManager;

    info!("Checking hooks status for runtime: {}", runtime);
    HooksManager::show_hooks_status(runtime)
}

/// Hidden callback used by installed Claude/Codex hooks. Writes the
/// per-runtime status file atomically and always returns success so a
/// transient failure never breaks the agent runtime's hook pipeline.
pub fn hook_status(_cli: &Cli, runtime: &str, status: &str) -> Result<()> {
    if let Err(error) = crate::hooks::HooksManager::write_runtime_status(runtime, status) {
        eprintln!("warp __hook-status: {error}");
    }
    Ok(())
}
