use crate::error::{GitWarpError, Result};
use dirs::config_dir;
use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Default terminal mode for worktree switching
    #[serde(default = "default_terminal_mode")]
    pub terminal_mode: String,

    /// Default worktree base directory
    pub worktrees_path: Option<PathBuf>,

    /// Whether to use Copy-on-Write by default
    #[serde(default = "default_true")]
    pub use_cow: bool,

    /// Whether to auto-confirm destructive operations
    #[serde(default)]
    pub auto_confirm: bool,

    /// Git configuration
    #[serde(default)]
    pub git: GitConfig,

    /// Process management settings
    #[serde(default)]
    pub process: ProcessConfig,

    /// Terminal integration settings
    #[serde(default)]
    pub terminal: TerminalConfig,

    /// Agent monitoring settings
    #[serde(default)]
    pub agent: AgentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfig {
    /// Default branch name (main, master, develop)
    #[serde(default = "default_main_branch")]
    pub default_branch: String,

    /// Branches that cleanup must never remove
    #[serde(default = "default_protected_branches")]
    pub protected_branches: Vec<String>,

    /// Whether to auto-fetch before operations
    #[serde(default = "default_true")]
    pub auto_fetch: bool,

    /// Whether to prune remote tracking branches
    #[serde(default = "default_true")]
    pub auto_prune: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    /// Whether to check for processes before cleanup
    #[serde(default = "default_true")]
    pub check_processes: bool,

    /// Whether to kill processes automatically
    #[serde(default)]
    pub auto_kill: bool,

    /// Grace period before force killing (seconds)
    #[serde(default = "default_kill_timeout")]
    pub kill_timeout: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    /// Preferred terminal application (iterm2, terminal, warp, auto)
    #[serde(default = "default_terminal_app")]
    pub app: String,

    /// Whether to activate new tabs/windows
    #[serde(default = "default_true")]
    pub auto_activate: bool,

    /// Custom init commands for new worktrees
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub init_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Enable agent monitoring
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Agent monitoring refresh rate (milliseconds)
    #[serde(default = "default_refresh_rate")]
    pub refresh_rate: u64,

    /// Maximum number of activities to track
    #[serde(default = "default_max_activities")]
    pub max_activities: usize,

    /// Enable Claude Code hooks integration
    #[serde(default = "default_true")]
    pub claude_hooks: bool,
}

// Default value functions
fn default_terminal_mode() -> String {
    "tab".to_string()
}

fn default_true() -> bool {
    true
}

fn default_main_branch() -> String {
    "main".to_string()
}

fn default_protected_branches() -> Vec<String> {
    vec![
        "main".to_string(),
        "master".to_string(),
        "develop".to_string(),
    ]
}

fn default_kill_timeout() -> u64 {
    5
}

fn default_terminal_app() -> String {
    "auto".to_string()
}

fn default_refresh_rate() -> u64 {
    1000
}

fn default_max_activities() -> usize {
    100
}

// Default implementations
impl Default for Config {
    fn default() -> Self {
        Self {
            terminal_mode: default_terminal_mode(),
            worktrees_path: None,
            use_cow: true,
            auto_confirm: false,
            git: GitConfig::default(),
            process: ProcessConfig::default(),
            terminal: TerminalConfig::default(),
            agent: AgentConfig::default(),
        }
    }
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            default_branch: default_main_branch(),
            protected_branches: default_protected_branches(),
            auto_fetch: true,
            auto_prune: true,
        }
    }
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            check_processes: true,
            auto_kill: false,
            kill_timeout: default_kill_timeout(),
        }
    }
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            app: default_terminal_app(),
            auto_activate: true,
            init_commands: Vec::new(),
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            refresh_rate: default_refresh_rate(),
            max_activities: default_max_activities(),
            claude_hooks: true,
        }
    }
}

impl Config {
    /// Create a configuration with intelligent defaults
    pub fn with_defaults() -> Self {
        Self::default()
    }

    /// Update configuration from environment variables
    pub fn apply_env_overrides(&mut self) {
        // Terminal mode
        if let Ok(mode) = std::env::var("GIT_WARP_TERMINAL_MODE") {
            self.terminal_mode = mode;
        }

        // Auto-confirm
        if let Ok(confirm) = std::env::var("GIT_WARP_AUTO_CONFIRM") {
            self.auto_confirm = confirm.parse().unwrap_or(false);
        }

        // CoW usage
        if let Ok(cow) = std::env::var("GIT_WARP_USE_COW") {
            self.use_cow = cow.parse().unwrap_or(true);
        }

        // Worktrees path
        if let Ok(path) = std::env::var("GIT_WARP_WORKTREES_PATH") {
            self.worktrees_path = Some(PathBuf::from(path));
        }
    }

    /// Generate a sample configuration file content
    pub fn sample_config() -> String {
        let config = Config::default();
        format!(
            r#"# Git-Warp Configuration
# This file configures git-warp behavior
# You can also set these values via environment variables with GIT_WARP_ prefix

# Terminal mode: tab, window, current, inplace, echo
terminal_mode = "{}"

# Use Copy-on-Write when available
use_cow = {}

# Auto-confirm destructive operations
auto_confirm = {}

# Custom worktrees directory (optional)
# worktrees_path = "/custom/path/to/worktrees"

[git]
# Default main branch name
default_branch = "{}"

# Branches cleanup must never remove
protected_branches = {:?}

# Auto-fetch before operations
auto_fetch = {}

# Auto-prune remote tracking branches
auto_prune = {}

[process]
# Check for processes before cleanup
check_processes = {}

# Auto-kill processes during cleanup
auto_kill = {}

# Grace period before force killing (seconds)
kill_timeout = {}

[terminal]
# Terminal app: auto, iterm2, terminal, warp
app = "{}"

# Auto-activate new tabs/windows
auto_activate = {}

# Commands to run after changing into a worktree
init_commands = []

[agent]
# Enable agent monitoring
enabled = {}

# Refresh rate for agent dashboard (milliseconds)
refresh_rate = {}

# Maximum activities to track
max_activities = {}

# Enable Claude Code hooks integration
claude_hooks = {}
"#,
            config.terminal_mode,
            config.use_cow,
            config.auto_confirm,
            config.git.default_branch,
            config.git.protected_branches,
            config.git.auto_fetch,
            config.git.auto_prune,
            config.process.check_processes,
            config.process.auto_kill,
            config.process.kill_timeout,
            config.terminal.app,
            config.terminal.auto_activate,
            config.agent.enabled,
            config.agent.refresh_rate,
            config.agent.max_activities,
            config.agent.claude_hooks,
        )
    }
}

pub struct ConfigManager {
    pub config: Config,
    pub config_path: PathBuf,
}

impl ConfigManager {
    /// Create a new config manager with default or loaded configuration
    pub fn new() -> Result<Self> {
        let config_path = get_config_path()?;
        let config = Self::load_config(&config_path)?;
        Ok(Self {
            config,
            config_path,
        })
    }

    /// Load configuration from file, environment, and defaults
    fn load_config(config_path: &PathBuf) -> Result<Config> {
        let figment = Figment::new()
            // Override with config file if it exists
            .merge(Toml::file(config_path))
            // Override with environment variables
            .merge(Env::prefixed("GIT_WARP_"));

        figment.extract().map_err(|e| {
            GitWarpError::ConfigError {
                message: format!("Failed to load configuration: {}", e),
            }
            .into()
        })
    }

    /// Get the current configuration
    pub fn get(&self) -> &Config {
        &self.config
    }

    /// Get a mutable reference to the configuration
    pub fn get_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    /// Save the configuration to file
    pub fn save(&self) -> Result<()> {
        self.save_config(&self.config_path, &self.config)
    }

    /// Save configuration to a specific path
    fn save_config(&self, path: &PathBuf, config: &Config) -> Result<()> {
        // Create config directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let toml_content =
            toml::to_string_pretty(config).map_err(|e| GitWarpError::ConfigError {
                message: format!("Failed to serialize configuration: {}", e),
            })?;

        fs::write(path, toml_content).map_err(|e| GitWarpError::ConfigError {
            message: format!("Failed to write config file: {}", e),
        })?;

        Ok(())
    }

    /// Get the path to the configuration file
    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    /// Create a default configuration file
    pub fn create_default_config(&self) -> Result<()> {
        let default_config = Config::default();
        self.save_config(&self.config_path, &default_config)
    }

    /// Check if configuration file exists
    pub fn config_exists(&self) -> bool {
        self.config_path.exists()
    }

    /// Generate and display sample configuration
    pub fn show_sample_config(&self) {
        println!("{}", Config::sample_config());
    }
}

/// Get the path to the configuration file
fn get_config_path() -> Result<PathBuf> {
    let config_dir = config_dir().ok_or_else(|| GitWarpError::ConfigError {
        message: "Could not determine config directory".to_string(),
    })?;

    Ok(config_dir.join("git-warp").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_config_defaults() {
        let config = Config::default();
        assert_eq!(config.terminal_mode, "tab");
        assert_eq!(config.use_cow, true);
        assert_eq!(config.auto_confirm, false);
        assert_eq!(config.git.default_branch, "main");
        assert_eq!(config.process.kill_timeout, 5);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();

        assert_eq!(config.terminal_mode, parsed.terminal_mode);
        assert_eq!(config.use_cow, parsed.use_cow);
        assert_eq!(config.git.default_branch, parsed.git.default_branch);
    }

    #[test]
    fn test_config_manager_creation() {
        let temp_dir = tempdir().unwrap();

        // Create config manager (will create default config)
        let manager = ConfigManager {
            config: Config::default(),
            config_path: temp_dir.path().join("config.toml"),
        };

        assert_eq!(manager.get().terminal_mode, "tab");
    }

    #[test]
    fn test_config_environment_overrides() {
        let mut config = Config::default();

        // Set environment variable
        unsafe {
            std::env::set_var("GIT_WARP_TERMINAL_MODE", "window");
            std::env::set_var("GIT_WARP_AUTO_CONFIRM", "true");
        }

        config.apply_env_overrides();

        assert_eq!(config.terminal_mode, "window");
        assert_eq!(config.auto_confirm, true);

        // Clean up
        unsafe {
            std::env::remove_var("GIT_WARP_TERMINAL_MODE");
            std::env::remove_var("GIT_WARP_AUTO_CONFIRM");
        }
    }

    #[test]
    fn test_sample_config_generation() {
        let sample = Config::sample_config();
        assert!(sample.contains("terminal_mode"));
        assert!(sample.contains("[git]"));
        assert!(sample.contains("[process]"));
        assert!(sample.contains("[agent]"));
    }
}
