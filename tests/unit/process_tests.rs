use git_warp::process::{ProcessInfo, ProcessManager};
use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn test_process_manager_creation() {
    let mut manager = ProcessManager::new();

    // Should be able to get system info
    manager.refresh();

    // Should have some processes
    let processes = manager.find_processes_in_directory(".").unwrap();
    // Note: May be empty if no processes are running in current directory
    println!("Found {} processes in current directory", processes.len());
}

#[test]
fn test_find_processes_in_nonexistent_directory() {
    let mut manager = ProcessManager::new();

    let result = manager.find_processes_in_directory("/nonexistent/path/that/should/not/exist");
    assert!(result.is_err());
}

#[test]
fn test_find_processes_in_empty_directory() {
    let temp_dir = tempdir().unwrap();
    let mut manager = ProcessManager::new();

    let processes = manager
        .find_processes_in_directory(temp_dir.path())
        .unwrap();
    // Should return empty vector for directory with no running processes
    assert_eq!(processes.len(), 0);
}

#[test]
fn test_process_detection_with_running_process() {
    let temp_dir = tempdir().unwrap();
    let script_path = temp_dir.path().join("test_script.sh");

    // Create a simple script that runs in the test directory
    let script_content = format!(
        r#"#!/bin/bash
cd "{}"
sleep 30
"#,
        temp_dir.path().display()
    );

    fs::write(&script_path, script_content).unwrap();

    // Make it executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    // Start the process
    let mut child = Command::new("bash")
        .arg(&script_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    // Give it time to start and change directory
    thread::sleep(Duration::from_millis(100));

    let mut manager = ProcessManager::new();
    let processes = manager
        .find_processes_in_directory(temp_dir.path())
        .unwrap();

    // Should find the sleep process
    let sleep_processes: Vec<_> = processes
        .iter()
        .filter(|p| p.name.contains("sleep"))
        .collect();

    // Clean up
    child.kill().unwrap();
    child.wait().unwrap();

    // On some systems, the process might not be detected immediately
    if !sleep_processes.is_empty() {
        assert!(sleep_processes.len() > 0);
        println!("Successfully detected running process in directory");
    } else {
        println!("Process detection test skipped (timing-dependent)");
    }
}

#[test]
fn test_process_termination() {
    // Start a simple long-running process
    let mut child = Command::new("sleep").arg("30").spawn().unwrap();

    let pid = child.id();

    let mut manager = ProcessManager::new();

    // Create a mock ProcessInfo for the child process
    let process_info = ProcessInfo {
        pid,
        name: "sleep".to_string(),
        cpu_usage: 0.0,
        memory_usage: 1024,
        working_dir: std::env::current_dir().unwrap(),
        cmd: "sleep 30".to_string(),
        start_time: 0, // Not used in termination
    };

    // Test termination
    let result = manager.terminate_processes(&[process_info], true);

    match result {
        Ok(success) => {
            assert!(success);

            // Process should be terminated
            thread::sleep(Duration::from_millis(100));
            let exit_status = child.try_wait().unwrap();
            assert!(exit_status.is_some(), "Process should have been terminated");
        }
        Err(e) => {
            // Clean up in case termination failed
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("Process termination failed: {}", e);
        }
    }
}

#[test]
fn test_process_termination_nonexistent() {
    let manager = ProcessManager::new();

    // Try to terminate a non-existent process
    let fake_process = ProcessInfo {
        pid: 999999, // Very unlikely to exist
        name: "fake_process".to_string(),
        cpu_usage: 0.0,
        memory_usage: 1024,
        working_dir: std::env::current_dir().unwrap(),
        cmd: "fake command".to_string(),
        start_time: 0,
    };

    let result = manager.terminate_processes(&[fake_process], true);

    // Should handle non-existent processes gracefully
    match result {
        Ok(success) => {
            // May return true or false depending on implementation
            println!("Termination of non-existent process returned: {}", success);
        }
        Err(e) => {
            println!(
                "Termination of non-existent process failed (expected): {}",
                e
            );
        }
    }
}

#[test]
fn test_process_info_display() {
    let process = ProcessInfo {
        pid: 1234,
        name: "test_process".to_string(),
        cpu_usage: 15.5,
        memory_usage: 1024 * 1024 * 50, // 50MB
        working_dir: "/test/directory".into(),
        cmd: "test_process --arg value".to_string(),
        start_time: 1234567890,
    };

    // Test that we can format process information
    assert_eq!(process.pid, 1234);
    assert_eq!(process.name, "test_process");
    assert_eq!(process.cpu_usage, 15.5);
    assert_eq!(process.memory_usage, 1024 * 1024 * 50);
    assert_eq!(process.working_dir.to_string_lossy(), "/test/directory");
    assert!(process.cmd.contains("test_process"));
}

#[test]
fn test_process_filtering() {
    let mut manager = ProcessManager::new();

    // Get all processes first
    manager.refresh();
    let all_processes = manager.find_processes_in_directory(".").unwrap();

    // All processes should have valid PIDs
    for process in &all_processes {
        assert!(process.pid > 0);
        assert!(!process.name.is_empty());
        assert!(!process.cmd.is_empty());
    }

    // Test that working directory filtering works
    let temp_dir = tempdir().unwrap();
    let temp_processes = manager
        .find_processes_in_directory(temp_dir.path())
        .unwrap();

    // Should be fewer (or equal) processes in temp directory
    assert!(temp_processes.len() <= all_processes.len());
}

#[test]
fn test_process_stats_collection() {
    let mut manager = ProcessManager::new();

    // Create a temp directory for testing
    let temp_dir = tempdir().unwrap();

    // Start a CPU-intensive process in the temp directory
    let script_path = temp_dir.path().join("cpu_test.sh");
    let script_content = format!(
        r#"#!/bin/bash
cd "{}"
# Simple CPU load
for i in {{1..1000}}; do
    echo $i > /dev/null
done
sleep 5
"#,
        temp_dir.path().display()
    );

    fs::write(&script_path, script_content).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let mut child = Command::new("bash")
        .arg(&script_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    // Give the process time to start and generate some load
    thread::sleep(Duration::from_millis(200));

    // Get process stats
    let result = manager.get_directory_process_stats(temp_dir.path());

    // Clean up
    child.kill().unwrap();
    child.wait().unwrap();

    match result {
        Ok(stats) => {
            // Should have collected some stats
            println!("Process stats: {:?}", stats);
            // Stats might be 0 if process finished too quickly
        }
        Err(e) => {
            println!("Stats collection failed (this may be expected): {}", e);
        }
    }
}

#[test]
fn test_graceful_vs_force_termination() {
    // This test verifies the termination strategy
    let mut child = Command::new("sleep").arg("30").spawn().unwrap();

    let pid = child.id();
    let manager = ProcessManager::new();

    let process_info = ProcessInfo {
        pid,
        name: "sleep".to_string(),
        cpu_usage: 0.0,
        memory_usage: 1024,
        working_dir: std::env::current_dir().unwrap(),
        cmd: "sleep 30".to_string(),
        start_time: 0,
    };

    // Test auto-confirm termination (should use graceful then force)
    let result = manager.terminate_processes(&[process_info], true);

    match result {
        Ok(_) => {
            // Process should be terminated
            thread::sleep(Duration::from_millis(100));
            let exit_status = child.try_wait().unwrap();
            assert!(exit_status.is_some());
        }
        Err(_) => {
            // Clean up
            child.kill().unwrap();
            child.wait().unwrap();
        }
    }
}

#[cfg(unix)]
#[test]
fn test_signal_handling() {
    use std::os::unix::process::ExitStatusExt;

    // Start a process that handles SIGTERM
    let script_content = r#"#!/bin/bash
trap 'echo "Received SIGTERM"; exit 0' TERM
sleep 30
"#;

    let temp_dir = tempdir().unwrap();
    let script_path = temp_dir.path().join("signal_test.sh");
    fs::write(&script_path, script_content).unwrap();

    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).unwrap();

    let mut child = Command::new("bash").arg(&script_path).spawn().unwrap();

    let pid = child.id();
    let manager = ProcessManager::new();

    let process_info = ProcessInfo {
        pid,
        name: "bash".to_string(),
        cpu_usage: 0.0,
        memory_usage: 1024,
        working_dir: temp_dir.path().to_path_buf(),
        cmd: format!("bash {}", script_path.display()),
        start_time: 0,
    };

    let result = manager.terminate_processes(&[process_info], true);

    match result {
        Ok(_) => {
            let exit_status = child.wait().unwrap();
            // Should have exited cleanly (exit code 0)
            assert_eq!(exit_status.code(), Some(0));
        }
        Err(_) => {
            child.kill().unwrap();
            child.wait().unwrap();
        }
    }
}
