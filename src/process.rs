use crate::error::{GitWarpError, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;
use sysinfo::{ProcessRefreshKind, System};

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cmd: String,
    pub working_dir: PathBuf,
    pub cpu_usage: f32,
    pub memory_usage: u64,
    // Captured for tests/future reporting; bin display path doesn't read it yet.
    #[allow(dead_code)]
    pub start_time: u64,
}

#[allow(dead_code)] // Public stats type used by tests/embedders.
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

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
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
            if let Some(cwd) = process.cwd()
                && cwd.starts_with(&target_path)
            {
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
        kill_timeout: Duration,
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

            if self.terminate_single_process(process.pid, kill_timeout) {
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

    /// Terminate a single process by PID with graceful fallback.
    /// `kill_timeout` caps the SIGTERM grace period; clamped to >= 100 ms.
    fn terminate_single_process(&self, pid: u32, kill_timeout: Duration) -> bool {
        let kill_timeout = kill_timeout.max(Duration::from_millis(100));
        #[cfg(unix)]
        {
            use nix::errno::Errno;
            use nix::sys::signal::{self, Signal};
            use nix::unistd::Pid;

            let nix_pid = Pid::from_raw(pid as i32);

            match signal::kill(nix_pid, Some(Signal::SIGTERM)) {
                Ok(()) => {}
                Err(Errno::ESRCH) => return true,
                Err(Errno::EPERM) => {
                    log::warn!("no permission to signal pid {pid}");
                    return false;
                }
                Err(err) => {
                    log::warn!("SIGTERM to pid {pid} failed: {err}");
                }
            }

            // Poll the process with signal 0 until it exits or the grace
            // budget expires. A fixed sleep would either rush a legitimate
            // slow exit into SIGKILL or block longer than needed when SIGTERM
            // is honored immediately.
            const POLL_INTERVAL: Duration = Duration::from_millis(50);

            let deadline = Instant::now() + kill_timeout;
            let mut still_running = true;
            loop {
                match signal::kill(nix_pid, None) {
                    Ok(()) => {}
                    Err(Errno::ESRCH) => {
                        still_running = false;
                        break;
                    }
                    Err(err) => {
                        log::warn!("liveness probe for pid {pid} failed: {err}");
                        break;
                    }
                }
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(POLL_INTERVAL);
            }

            if !still_running {
                return true;
            }

            match signal::kill(nix_pid, Some(Signal::SIGKILL)) {
                Ok(()) | Err(Errno::ESRCH) => true,
                Err(err) => {
                    log::warn!("SIGKILL to pid {pid} failed: {err}");
                    false
                }
            }
        }

        #[cfg(windows)]
        {
            let _ = kill_timeout; // taskkill is immediate; timeout unused on Windows.
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
            let _ = kill_timeout;
            false
        }
    }

    /// Check if any processes are running in the directory
    pub fn has_processes_in_directory<P: AsRef<Path>>(&mut self, path: P) -> Result<bool> {
        let processes = self.find_processes_in_directory(path)?;
        Ok(!processes.is_empty())
    }

    /// Get detailed process statistics for a directory
    #[allow(dead_code)] // Public helper used by tests/embedders.
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
    #[allow(dead_code)] // Public helper kept for embedders.
    pub fn kill_directory_processes<P: AsRef<Path>>(
        &mut self,
        path: P,
        auto_confirm: bool,
        kill_timeout: Duration,
    ) -> Result<bool> {
        let processes = self.find_processes_in_directory(path)?;

        if processes.is_empty() {
            println!("✨ No processes found in directory");
            return Ok(true);
        }

        self.terminate_processes(&processes, auto_confirm, kill_timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_process_manager_creation() {
        let _manager = ProcessManager::new();
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

    #[cfg(unix)]
    #[test]
    fn terminate_single_process_handles_missing_pid() {
        use std::process::Command as StdCommand;

        // Spawn a short-lived child, wait for it to exit, then reap it. The
        // resulting PID is guaranteed gone so signal::kill should report
        // ESRCH, which the function maps to success.
        let mut child = StdCommand::new("true").spawn().expect("spawn true");
        let pid = child.id();
        let _ = child.wait();

        let manager = ProcessManager::new();
        let ok = manager.terminate_single_process(pid, Duration::from_millis(200));

        assert!(ok, "ESRCH should be treated as success");
    }

    #[cfg(unix)]
    #[test]
    fn terminate_single_process_respects_kill_timeout() {
        use std::process::Command as StdCommand;

        // Spawn a child that ignores SIGTERM so we exercise the SIGKILL path
        // gated on kill_timeout. `sh -c "trap '' TERM; sleep 30"` will only
        // exit on SIGKILL.
        let mut child = StdCommand::new("sh")
            .args(["-c", "trap '' TERM; sleep 30"])
            .spawn()
            .expect("spawn sleep child");
        let pid = child.id();

        let manager = ProcessManager::new();
        let start = Instant::now();
        let ok = manager.terminate_single_process(pid, Duration::from_millis(200));
        let elapsed = start.elapsed();

        assert!(ok, "terminate_single_process should report success");
        // SIGTERM grace is 200 ms; total wall time must be well under the
        // 30 s sleep. Cap generously to avoid CI flake.
        assert!(
            elapsed < Duration::from_secs(5),
            "expected fast SIGKILL fallback, took {:?}",
            elapsed
        );

        // Reap the child so it doesn't leak as a zombie.
        let _ = child.wait();
    }
}
