use anyhow::Result;
use log::info;

use crate::cli::Cli;

pub fn run(cli: &Cli) -> Result<()> {
    use crate::agents::AgentDiscovery;
    use crate::config::ConfigManager;
    use crate::git::GitRepository;
    use crate::tui::AgentsDashboard;

    use super::util::{agent_monitored_paths, not_in_git_repo_error};

    info!("Starting agents dashboard");
    if cli.dry_run {
        println!("Would start agents dashboard");
        return Ok(());
    }

    let git_repo = GitRepository::find().map_err(|_| not_in_git_repo_error())?;
    let config_manager = ConfigManager::new()?;
    let agent_config = &config_manager.get().agent;
    if !agent_config.enabled {
        println!("🚫 Agent monitoring is disabled (agent.enabled=false in config).");
        return Ok(());
    }
    let dashboard = AgentsDashboard::with_refresh_rate(
        AgentDiscovery::with_max_history_sessions(
            agent_monitored_paths(&git_repo)?,
            agent_config.max_activities,
        ),
        agent_config.refresh_rate,
    );
    dashboard.run()
}
