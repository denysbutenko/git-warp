use anyhow::Result;
use log::info;
use std::path::Path;
use std::process::Command;

use crate::cli::Cli;

pub fn run(cli: &Cli, show: bool, edit: bool, interactive: bool) -> Result<()> {
    use crate::config::ConfigManager;
    use crate::tui::ConfigTui;

    info!("Config command");
    if cli.dry_run {
        println!("Would manage configuration");
        return Ok(());
    }

    if interactive {
        return ConfigTui::new().run();
    }

    let config_manager = ConfigManager::new()?;

    if show {
        // Show current configuration
        println!("📋 Current Git-Warp Configuration:");
        println!("Config file: {}", config_manager.config_path().display());
        println!();

        let config = config_manager.get();

        println!("🖥️  Terminal Settings:");
        println!("  Mode: {}", config.terminal_mode);
        println!("  Use CoW: {}", config.use_cow);
        println!("  Auto-confirm: {}", config.auto_confirm);
        if let Some(path) = &config.worktrees_path {
            println!("  Worktrees path: {}", path.display());
        }
        println!();

        println!("🔧 Git Settings:");
        println!("  Default branch: {}", config.git.default_branch);
        println!("  Protected branches: {:?}", config.git.protected_branches);
        println!("  Auto-fetch: {}", config.git.auto_fetch);
        println!("  Auto-prune: {}", config.git.auto_prune);
        println!();

        println!("⚙️  Process Settings:");
        println!("  Check processes: {}", config.process.check_processes);
        println!("  Auto-kill: {}", config.process.auto_kill);
        println!("  Kill timeout: {}s", config.process.kill_timeout);
        println!();

        println!("🖥️  Terminal Integration:");
        println!("  App: {}", config.terminal.app);
        if let Err(err) = crate::terminal::validate_terminal_app(&config.terminal.app) {
            eprintln!("  ⚠️  {}", err);
        }
        println!("  Auto-activate: {}", config.terminal.auto_activate);
        println!("  Init commands: {:?}", config.terminal.init_commands);
        println!();

        println!("🤖 Agent Settings:");
        println!("  Enabled: {}", config.agent.enabled);
        println!("  Refresh rate: {}ms", config.agent.refresh_rate);
        println!("  Max activities: {}", config.agent.max_activities);
    } else if edit {
        if !config_manager.config_exists() {
            config_manager.create_default_config()?;
            println!(
                "✅ Created default configuration at: {}",
                config_manager.config_path().display()
            );
        }

        open_in_editor(config_manager.config_path())?;
    } else {
        // Show help for config command
        println!("⚙️  Configuration Management");
        println!();
        println!("Usage:");
        println!("  warp config --show          Show current configuration");
        println!("  warp config --edit          Open configuration in your editor");
        println!("  warp config --interactive   Launch the interactive editor TUI");
        println!();
        println!("Configuration file location:");
        println!("  {}", config_manager.config_path().display());
        println!();
        println!("Environment variables (GIT_WARP_ prefix):");
        println!("  GIT_WARP_TERMINAL_MODE=tab|window|current|inplace|echo");
        println!("  GIT_WARP_USE_COW=true|false");
        println!("  GIT_WARP_AUTO_CONFIRM=true|false");
        println!("  GIT_WARP_WORKTREES_PATH=/custom/path");
        println!();
        println!("Nested sections use `__` (double underscore) as the separator:");
        println!("  GIT_WARP_GIT__DEFAULT_BRANCH=develop");
        println!("  GIT_WARP_GIT__AUTO_FETCH=false");
        println!("  GIT_WARP_PROCESS__AUTO_KILL=true");
        println!("  GIT_WARP_TERMINAL__APP=iterm2");
        println!("  GIT_WARP_AGENT__REFRESH_RATE=2000");
        println!("  GIT_WARP_POST_CREATE__AUTO_INSTALL=false");
    }

    Ok(())
}

fn open_in_editor(path: &Path) -> Result<()> {
    let editor = std::env::var("VISUAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("EDITOR")
                .ok()
                .filter(|value| !value.trim().is_empty())
        });

    if let Some(editor) = editor {
        let mut parts = editor.split_whitespace();
        let program = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("Invalid editor command"))?;
        let status = Command::new(program)
            .args(parts)
            .arg(path)
            .status()
            .map_err(|err| anyhow::anyhow!("Failed to launch editor '{}': {}", editor, err))?;

        if status.success() {
            return Ok(());
        }

        return Err(anyhow::anyhow!(
            "Editor '{}' exited with status {:?}",
            editor,
            status.code()
        ));
    }

    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .args(["-t"])
            .arg(path)
            .status()
            .map_err(|err| anyhow::anyhow!("Failed to open config file: {}", err))?;

        if status.success() {
            return Ok(());
        }
    }

    Err(anyhow::anyhow!(
        "No editor configured. Set $VISUAL or $EDITOR to use `warp config --edit`"
    ))
}
