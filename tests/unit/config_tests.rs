use git_warp::config::{
    AgentConfig, Config, ConfigManager, GitConfig, ProcessConfig, TerminalConfig,
};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_config_defaults() {
    let config = Config::default();
    assert_eq!(config.terminal_mode, "tab");
    assert!(config.use_cow);
    assert!(!config.auto_confirm);
    assert_eq!(config.git.default_branch, "main");
    assert!(config.git.auto_fetch);
    assert!(config.git.auto_prune);
    assert!(config.process.check_processes);
    assert!(!config.process.auto_kill);
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
fn test_config_with_custom_values() {
    let config = Config {
        terminal_mode: "window".to_string(),
        worktrees_path: Some("/custom/path".into()),
        use_cow: false,
        auto_confirm: true,
        git: GitConfig {
            default_branch: "develop".to_string(),
            auto_fetch: false,
            auto_prune: false,
        },
        process: ProcessConfig {
            check_processes: false,
            auto_kill: true,
            kill_timeout: 10,
        },
        terminal: TerminalConfig {
            app: "iterm2".to_string(),
            auto_activate: false,
            init_commands: vec!["npm install".to_string()],
        },
        agent: AgentConfig {
            enabled: false,
            refresh_rate: 2000,
            max_activities: 50,
            claude_hooks: false,
        },
    };

    let toml_str = toml::to_string(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();

    assert_eq!(config.terminal_mode, parsed.terminal_mode);
    assert_eq!(config.use_cow, parsed.use_cow);
    assert_eq!(config.git.default_branch, parsed.git.default_branch);
    assert!(!parsed.git.auto_fetch);
    assert!(parsed.process.auto_kill);
}

#[test]
fn test_config_manager_creation() {
    // Test that ConfigManager creates default config when none exists
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let manager = ConfigManager {
        config: Config::default(),
        config_path: config_path.clone(),
    };

    assert_eq!(manager.get().terminal_mode, "tab");
    assert!(!manager.config_exists());
}

#[test]
fn test_config_save_and_load() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let mut config = Config::default();
    config.terminal_mode = "window".to_string();
    config.use_cow = false;

    let manager = ConfigManager {
        config: config.clone(),
        config_path: config_path.clone(),
    };

    manager.save().unwrap();
    assert!(config_path.exists());

    // Read back the config
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("terminal_mode = \"window\""));
    assert!(content.contains("use_cow = false"));
}

#[test]
fn test_sample_config_generation() {
    let sample = Config::sample_config();

    assert!(sample.contains("terminal_mode"));
    assert!(sample.contains("[git]"));
    assert!(sample.contains("[process]"));
    assert!(sample.contains("[terminal]"));
    assert!(sample.contains("[agent]"));
    assert!(sample.contains("# Git-Warp Configuration"));
}

#[test]
fn test_environment_variable_parsing() {
    // Test that the figment configuration would parse environment variables
    // This simulates what would happen in load_config
    unsafe {
        std::env::set_var("GIT_WARP_TERMINAL_MODE", "window");
        std::env::set_var("GIT_WARP_AUTO_CONFIRM", "true");
        std::env::set_var("GIT_WARP_USE_COW", "false");
    }

    let figment = figment::Figment::new().merge(figment::providers::Env::prefixed("GIT_WARP_"));

    let config: Config = figment.extract().unwrap();
    assert_eq!(config.terminal_mode, "window");
    assert!(config.auto_confirm);
    assert!(!config.use_cow);

    // Clean up
    unsafe {
        std::env::remove_var("GIT_WARP_TERMINAL_MODE");
        std::env::remove_var("GIT_WARP_AUTO_CONFIRM");
        std::env::remove_var("GIT_WARP_USE_COW");
    }
}

#[test]
fn test_nested_config_structures() {
    let config = Config {
        terminal: TerminalConfig {
            app: "terminal".to_string(),
            auto_activate: false,
            init_commands: vec!["echo hello".to_string(), "ls -la".to_string()],
        },
        ..Default::default()
    };

    let toml_str = toml::to_string(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();

    assert_eq!(parsed.terminal.app, "terminal");
    assert!(!parsed.terminal.auto_activate);
    assert_eq!(parsed.terminal.init_commands.len(), 2);
    assert_eq!(parsed.terminal.init_commands[0], "echo hello");
}

#[test]
fn test_config_validation() {
    // Test that invalid values are handled gracefully
    let toml_str = r#"
terminal_mode = "invalid_mode"
use_cow = true
auto_confirm = false

[git]
default_branch = ""
auto_fetch = true
"#;

    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.terminal_mode, "invalid_mode"); // Should preserve invalid values
    assert_eq!(config.git.default_branch, ""); // Should preserve empty strings
}
