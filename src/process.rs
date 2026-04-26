use crate::error::{GitWarpError, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, System};

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cmd: String,
    pub working_dir: PathBuf,
    pub cpu_usage: f32,
    pub memory_usage: u64,
    pub start_time: u64,
}

#[derive(Debug)]
pub struct ProcessStats {
    pub total_count: usize,
    pub total_memory: u64,
    pub total_cpu: f32,
    pub high_cpu_count: usize,
    pub processes: Vec<ProcessInfo>,
}

pub struct ProcessManager {
    system: System,
}

impl ProcessManager {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_all();
        Self { system }
    }

    /// Refresh process information
    pub fn refresh(&mut self) {
        self.system
            .refresh_processes_specifics(ProcessRefreshKind::new());
    }

    /// Find all processes running in a specific directory
    pub fn find_processes_in_directory<P: AsRef<Path>>(
        &mut self,
        path: P,
    ) -> Result<Vec<ProcessInfo>> {
        let requested_path = path.as_ref();
        let target_path =
            requested_path
                .canonicalize()
                .map_err(|_| GitWarpError::WorktreeNotFound {
                    path: requested_path.display().to_string(),
                })?;

        self.refresh();
        let mut processes = Vec::new();

        for (pid, process) in self.system.processes() {
            if let Some(cwd) = process.cwd() {
                if cwd.starts_with(&target_path) {
                    processes.push(ProcessInfo {
                        pid: pid.as_u32(),
                        name: process.name().to_string(),
                        cmd: process.cmd().join(" "),
                        working_dir: cwd.to_path_buf(),
                        cpu_usage: process.cpu_usage(),
                        memory_usage: process.memory(),
                        start_time: process.start_time(),
                    });
                }
            }
        }

        // Sort by CPU usage (most active first)
        processes.sort_by(|a, b| {
            b.cpu_usage
                .partial_cmp(&a.cpu_usage)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(processes)
    }

    /// Terminate processes with user confirmation and progress feedback
    pub fn terminate_processes(
        &self,
        processes: &[ProcessInfo],
        auto_confirm: bool,
    ) -> Result<bool> {
        if processes.is_empty() {
            return Ok(true);
        }

        self.display_process_list(processes);

        if !auto_confirm && !self.confirm_termination()? {
            println!("❌ Process termination cancelled");
            return Ok(false);
        }

        let mut success_count = 0;
        let mut failed_count = 0;

        for process in processes {
            println!("🔪 Terminating PID {}: {}", process.pid, process.name);

            if self.terminate_single_process(process.pid) {
                success_count += 1;
                println!("  ✅ Terminated successfully");
            } else {
                failed_count += 1;
                println!("  ❌ Failed to terminate");
            }
        }

        println!(
            "\n📊 Process termination complete: {} succeeded, {} failed",
            success_count, failed_count
        );
        Ok(failed_count == 0)
    }

    fn display_process_list(&self, processes: &[ProcessInfo]) {
        println!("\n⚠️  Found {} processes in worktree:", processes.len());
        for process in processes {
            let memory_mb = process.memory_usage / 1024 / 1024;
            println!(
                "  • PID {}: {} (CPU: {:.1}%, Mem: {}MB)",
                process.pid, process.name, process.cpu_usage, memory_mb
            );
            println!("    Working dir: {}", process.working_dir.display());
            if !process.cmd.is_empty() {
                println!("    Command: {}", process.cmd);
            }
        }
    }

    fn confirm_termination(&self) -> Result<bool> {
        println!("\n❓ Terminate these processes? [y/N]: ");
        use std::io::{self, Write};
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        Ok(input.trim().to_lowercase().starts_with('y'))
    }

    /// Terminate a single process by PID with graceful fallback
    fn terminate_single_process(&self, pid: u32) -> bool {
        #[cfg(unix)]
        {
            use std::process::Command;

            // Try graceful termination first (SIGTERM)
            let graceful_result = Command::new("kill")
                .arg("-TERM")
                .arg(pid.to_string())
                .output();

            match graceful_result {
                Ok(output) if output.status.success() => {
                    // Wait for graceful shutdown
                    std::thread::sleep(Duration::from_millis(2000));

                    // Check if process is still running
                    let check_result = Command::new("kill").arg("-0").arg(pid.to_string()).output();

                    match check_result {
                        Ok(output) if output.status.success() => {
                            // Process still running, force kill
                            let force_result = Command::new("kill")
                                .arg("-KILL")
                                .arg(pid.to_string())
                                .output();
                            force_result.map(|o| o.status.success()).unwrap_or(false)
                        }
                        _ => true, // Process gracefully terminated
                    }
                }
                _ => {
                    // Graceful termination failed, force kill immediately
                    let force_result = Command::new("kill")
                        .arg("-KILL")
                        .arg(pid.to_string())
                        .output();
                    force_result.map(|o| o.status.success()).unwrap_or(false)
                }
            }
        }

        #[cfg(windows)]
        {
            use std::process::Command;

            let result = Command::new("taskkill")
                .arg("/PID")
                .arg(pid.to_string())
                .arg("/F")
                .output();

            result.map(|o| o.status.success()).unwrap_or(false)
        }

        #[cfg(not(any(unix, windows)))]
        {
            false
        }
    }

    /// Check if any processes are running in the directory
    pub fn has_processes_in_directory<P: AsRef<Path>>(&mut self, path: P) -> Result<bool> {
        let processes = self.find_processes_in_directory(path)?;
        Ok(!processes.is_empty())
    }

    /// Get detailed process statistics for a directory
    pub fn get_directory_process_stats<P: AsRef<Path>>(&mut self, path: P) -> Result<ProcessStats> {
        let processes = self.find_processes_in_directory(path)?;

        let total_count = processes.len();
        let total_memory = processes.iter().map(|p| p.memory_usage).sum::<u64>();
        let total_cpu = processes.iter().map(|p| p.cpu_usage).sum::<f32>();
        let high_cpu_count = processes.iter().filter(|p| p.cpu_usage > 10.0).count();

        Ok(ProcessStats {
            total_count,
            total_memory,
            total_cpu,
            high_cpu_count,
            processes,
        })
    }

    /// Kill all processes in a directory with confirmation
    pub fn kill_directory_processes<P: AsRef<Path>>(
        &mut self,
        path: P,
        auto_confirm: bool,
    ) -> Result<bool> {
        let processes = self.find_processes_in_directory(path)?;

        if processes.is_empty() {
            println!("✨ No processes found in directory");
            return Ok(true);
        }

        self.terminate_processes(&processes, auto_confirm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_process_manager_creation() {
        let manager = ProcessManager::new();
        // Just verify we can create a process manager
    }

    #[test]
    fn test_find_processes_empty_directory() {
        let temp_dir = tempdir().unwrap();
        let mut manager = ProcessManager::new();

        let result = manager.find_processes_in_directory(temp_dir.path());
        assert!(result.is_ok());
        // Most likely no processes will be running in a temporary directory
    }

    #[test]
    fn test_process_info_fields() {
        let process = ProcessInfo {
            pid: 12345,
            name: "test_process".to_string(),
            cmd: "test command".to_string(),
            working_dir: PathBuf::from("/test"),
            cpu_usage: 5.5,
            memory_usage: 1024 * 1024, // 1MB
            start_time: 1234567890,
        };

        assert_eq!(process.pid, 12345);
        assert_eq!(process.name, "test_process");
        assert_eq!(process.cpu_usage, 5.5);
        assert_eq!(process.memory_usage, 1024 * 1024);
    }

    #[test]
    fn test_has_processes_in_directory() {
        let temp_dir = tempdir().unwrap();
        let mut manager = ProcessManager::new();

        let result = manager.has_processes_in_directory(temp_dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_stats() {
        let processes = vec![
            ProcessInfo {
                pid: 1,
                name: "proc1".to_string(),
                cmd: "test1".to_string(),
                working_dir: PathBuf::from("/test"),
                cpu_usage: 15.0,
                memory_usage: 1024,
                start_time: 1000,
            },
            ProcessInfo {
                pid: 2,
                name: "proc2".to_string(),
                cmd: "test2".to_string(),
                working_dir: PathBuf::from("/test"),
                cpu_usage: 5.0,
                memory_usage: 2048,
                start_time: 1100,
            },
        ];

        let stats = ProcessStats {
            total_count: processes.len(),
            total_memory: processes.iter().map(|p| p.memory_usage).sum(),
            total_cpu: processes.iter().map(|p| p.cpu_usage).sum(),
            high_cpu_count: processes.iter().filter(|p| p.cpu_usage > 10.0).count(),
            processes,
        };

        assert_eq!(stats.total_count, 2);
        assert_eq!(stats.total_memory, 3072);
        assert_eq!(stats.total_cpu, 20.0);
        assert_eq!(stats.high_cpu_count, 1);
    }
}
