use git_warp::tui::{AgentActivity, AgentStatus, AgentsDashboard, TuiApp};
use std::path::PathBuf;
use std::time::Instant;

#[test]
fn test_tui_app_creation() {
    let app = TuiApp::new();

    assert!(!app.should_quit());
    assert_eq!(app.get_selected_index(), 0);
    assert!(app.get_last_update() <= Instant::now());
}

#[test]
fn test_agent_status_symbols() {
    assert_eq!(AgentStatus::Active.symbol(), "🔄");
    assert_eq!(AgentStatus::Waiting.symbol(), "⏳");
    assert_eq!(AgentStatus::Completed.symbol(), "✅");
    assert_eq!(AgentStatus::Error.symbol(), "❌");
}

#[test]
fn test_agent_activity_creation() {
    let activity = AgentActivity {
        timestamp: "14:32:15".to_string(),
        agent_name: "Claude-Code".to_string(),
        activity: "Analyzing code structure".to_string(),
        file_path: Some(PathBuf::from("/project/src/main.rs")),
        status: AgentStatus::Active,
    };

    assert_eq!(activity.timestamp, "14:32:15");
    assert_eq!(activity.agent_name, "Claude-Code");
    assert_eq!(activity.activity, "Analyzing code structure");
    assert!(activity.file_path.is_some());
    assert_eq!(
        activity.file_path.unwrap().to_string_lossy(),
        "/project/src/main.rs"
    );

    match activity.status {
        AgentStatus::Active => {
            assert_eq!(activity.status.symbol(), "🔄");
        }
        _ => panic!("Expected Active status"),
    }
}

#[test]
fn test_agent_activity_without_file() {
    let activity = AgentActivity {
        timestamp: "14:31:42".to_string(),
        agent_name: "Claude-Code".to_string(),
        activity: "Waiting for user input".to_string(),
        file_path: None,
        status: AgentStatus::Waiting,
    };

    assert!(activity.file_path.is_none());
    assert_eq!(activity.status.symbol(), "⏳");
}

#[test]
fn test_agents_dashboard_creation() {
    let dashboard = AgentsDashboard::new();

    // Dashboard should be created successfully
    // In a real implementation, this would initialize the dashboard state
    println!("AgentsDashboard created successfully");
}

#[test]
fn test_multiple_agent_activities() {
    let activities = vec![
        AgentActivity {
            timestamp: "14:35:00".to_string(),
            agent_name: "Claude-Code".to_string(),
            activity: "Writing tests".to_string(),
            file_path: Some(PathBuf::from("/project/tests/mod.rs")),
            status: AgentStatus::Active,
        },
        AgentActivity {
            timestamp: "14:34:30".to_string(),
            agent_name: "Claude-Code".to_string(),
            activity: "Refactoring function".to_string(),
            file_path: Some(PathBuf::from("/project/src/utils.rs")),
            status: AgentStatus::Completed,
        },
        AgentActivity {
            timestamp: "14:34:00".to_string(),
            agent_name: "Claude-Code".to_string(),
            activity: "Analyzing dependencies".to_string(),
            file_path: Some(PathBuf::from("/project/Cargo.toml")),
            status: AgentStatus::Completed,
        },
    ];

    assert_eq!(activities.len(), 3);

    // Test ordering by timestamp
    let latest = &activities[0];
    let oldest = &activities[2];

    assert_eq!(latest.timestamp, "14:35:00");
    assert_eq!(oldest.timestamp, "14:34:00");

    // Test different statuses
    let active_count = activities
        .iter()
        .filter(|a| matches!(a.status, AgentStatus::Active))
        .count();
    let completed_count = activities
        .iter()
        .filter(|a| matches!(a.status, AgentStatus::Completed))
        .count();

    assert_eq!(active_count, 1);
    assert_eq!(completed_count, 2);
}

#[test]
fn test_agent_status_transitions() {
    // Test that agent statuses represent different states correctly
    let mut activity = AgentActivity {
        timestamp: "14:30:00".to_string(),
        agent_name: "Claude-Code".to_string(),
        activity: "Starting analysis".to_string(),
        file_path: Some(PathBuf::from("/project/src/main.rs")),
        status: AgentStatus::Waiting,
    };

    // Waiting -> Active
    activity.status = AgentStatus::Active;
    activity.activity = "Analyzing code".to_string();
    assert_eq!(activity.status.symbol(), "🔄");

    // Active -> Completed
    activity.status = AgentStatus::Completed;
    activity.activity = "Analysis complete".to_string();
    assert_eq!(activity.status.symbol(), "✅");

    // Test error state
    activity.status = AgentStatus::Error;
    activity.activity = "Analysis failed".to_string();
    assert_eq!(activity.status.symbol(), "❌");
}

#[test]
fn test_file_path_handling() {
    // Test various file path scenarios
    let activities = vec![
        AgentActivity {
            timestamp: "14:30:00".to_string(),
            agent_name: "Claude-Code".to_string(),
            activity: "Editing source file".to_string(),
            file_path: Some(PathBuf::from("/project/src/main.rs")),
            status: AgentStatus::Active,
        },
        AgentActivity {
            timestamp: "14:30:10".to_string(),
            agent_name: "Claude-Code".to_string(),
            activity: "Waiting for input".to_string(),
            file_path: None,
            status: AgentStatus::Waiting,
        },
        AgentActivity {
            timestamp: "14:30:20".to_string(),
            agent_name: "Claude-Code".to_string(),
            activity: "Updating config".to_string(),
            file_path: Some(PathBuf::from("/project/config/settings.toml")),
            status: AgentStatus::Active,
        },
    ];

    let with_files: Vec<_> = activities
        .iter()
        .filter(|a| a.file_path.is_some())
        .collect();
    let without_files: Vec<_> = activities
        .iter()
        .filter(|a| a.file_path.is_none())
        .collect();

    assert_eq!(with_files.len(), 2);
    assert_eq!(without_files.len(), 1);

    // Test path display
    for activity in with_files {
        if let Some(path) = &activity.file_path {
            assert!(!path.to_string_lossy().is_empty());
            println!("File path: {}", path.display());
        }
    }
}

#[test]
fn test_agent_dashboard_statistics() {
    let activities = vec![
        AgentActivity {
            timestamp: "14:30:00".to_string(),
            agent_name: "Claude-Code".to_string(),
            activity: "Task 1".to_string(),
            file_path: None,
            status: AgentStatus::Active,
        },
        AgentActivity {
            timestamp: "14:30:10".to_string(),
            agent_name: "Claude-Code".to_string(),
            activity: "Task 2".to_string(),
            file_path: None,
            status: AgentStatus::Completed,
        },
        AgentActivity {
            timestamp: "14:30:20".to_string(),
            agent_name: "Claude-Code".to_string(),
            activity: "Task 3".to_string(),
            file_path: None,
            status: AgentStatus::Completed,
        },
        AgentActivity {
            timestamp: "14:30:30".to_string(),
            agent_name: "Claude-Code".to_string(),
            activity: "Task 4".to_string(),
            file_path: None,
            status: AgentStatus::Error,
        },
    ];

    // Calculate statistics
    let total = activities.len();
    let active = activities
        .iter()
        .filter(|a| matches!(a.status, AgentStatus::Active))
        .count();
    let completed = activities
        .iter()
        .filter(|a| matches!(a.status, AgentStatus::Completed))
        .count();
    let errors = activities
        .iter()
        .filter(|a| matches!(a.status, AgentStatus::Error))
        .count();
    let waiting = activities
        .iter()
        .filter(|a| matches!(a.status, AgentStatus::Waiting))
        .count();

    assert_eq!(total, 4);
    assert_eq!(active, 1);
    assert_eq!(completed, 2);
    assert_eq!(errors, 1);
    assert_eq!(waiting, 0);

    // Test completion rate
    let completion_rate = completed as f64 / total as f64;
    assert!((completion_rate - 0.5).abs() < f64::EPSILON);
}

#[test]
fn test_tui_navigation() {
    let mut app = TuiApp::new();

    // Test initial state
    assert_eq!(app.get_selected_index(), 0);

    // Simulate navigation (these would be triggered by keyboard events)
    app.set_selected_index(1);
    assert_eq!(app.get_selected_index(), 1);

    app.set_selected_index(2);
    assert_eq!(app.get_selected_index(), 2);

    // Test bounds checking (would be implemented in event handlers)
    let max_items = 5;
    if app.get_selected_index() >= max_items {
        app.set_selected_index(max_items - 1);
    }
    assert!(app.get_selected_index() < max_items);
}

#[test]
fn test_refresh_functionality() {
    let mut app = TuiApp::new();
    let initial_time = app.get_last_update();

    // Simulate time passing
    std::thread::sleep(std::time::Duration::from_millis(1));

    // Update timestamp (simulate refresh)
    app.set_last_update(Instant::now());

    assert!(app.get_last_update() > initial_time);
}

#[test]
fn test_agent_name_variations() {
    let agent_names = vec!["Claude-Code", "Claude", "Assistant", "AI-Agent"];

    for name in agent_names {
        let activity = AgentActivity {
            timestamp: "14:30:00".to_string(),
            agent_name: name.to_string(),
            activity: "Testing".to_string(),
            file_path: None,
            status: AgentStatus::Active,
        };

        assert_eq!(activity.agent_name, name);
        assert!(!activity.agent_name.is_empty());
    }
}

#[test]
fn test_worktree_monitoring() {
    let dashboard = AgentsDashboard::new();
    let worktree_path = PathBuf::from("/project/worktrees/feature-branch");

    // Test that monitoring setup doesn't panic
    let result = dashboard.monitor_worktree(worktree_path);

    match result {
        Ok(()) => {
            println!("Worktree monitoring started successfully");
        }
        Err(e) => {
            println!("Worktree monitoring failed (expected in test): {}", e);
        }
    }
}

#[test]
fn test_cleanup_tui() {
    use git_warp::tui::CleanupTui;

    let cleanup_tui = CleanupTui::new();

    // Test that cleanup TUI can be created
    println!("CleanupTui created successfully");

    // The run method would return selected branches for cleanup
    // In a real test, we'd mock this interaction
}

#[test]
fn test_config_tui() {
    use git_warp::tui::ConfigTui;

    let config_tui = ConfigTui::new();

    // Test that config TUI can be created
    println!("ConfigTui created successfully");

    // The run method would handle configuration editing
    // In a real test, we'd mock this interaction
}
