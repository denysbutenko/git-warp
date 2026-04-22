use crate::error::{GitWarpError, Result};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone)]
pub enum TerminalMode {
    Tab,
    Window,
    InPlace,
    Echo,
}

impl TerminalMode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "tab" => Some(Self::Tab),
            "window" => Some(Self::Window),
            "inplace" => Some(Self::InPlace),
            "echo" => Some(Self::Echo),
            _ => None,
        }
    }
}

pub trait Terminal {
    fn open_tab(&self, path: &Path, session_id: Option<&str>) -> Result<()>;
    fn open_window(&self, path: &Path, session_id: Option<&str>) -> Result<()>;
    fn switch_to_directory(&self, path: &Path) -> Result<()>;
    fn echo_commands(&self, path: &Path) -> Result<()>;
    fn is_supported(&self) -> bool;
}

#[cfg(target_os = "macos")]
pub struct ITerm2;

#[cfg(target_os = "macos")]
impl Terminal for ITerm2 {
    fn open_tab(&self, path: &Path, _session_id: Option<&str>) -> Result<()> {
        let script = format!(
            r#"
tell application "iTerm"
    tell current window
        create tab with default profile
        tell current tab
            tell current session
                write text "cd '{}'"
            end tell
        end tell
    end tell
end tell
"#,
            path.display()
        );

        self.run_applescript(&script)
    }

    fn open_window(&self, path: &Path, _session_id: Option<&str>) -> Result<()> {
        let script = format!(
            r#"
tell application "iTerm"
    create window with default profile
    tell current window
        tell current tab
            tell current session
                write text "cd '{}'"
            end tell
        end tell
    end tell
end tell
"#,
            path.display()
        );

        self.run_applescript(&script)
    }

    fn switch_to_directory(&self, path: &Path) -> Result<()> {
        println!("cd '{}'", path.display());
        Ok(())
    }

    fn echo_commands(&self, path: &Path) -> Result<()> {
        println!("# Navigate to worktree:");
        println!("cd '{}'", path.display());
        Ok(())
    }

    fn is_supported(&self) -> bool {
        // Check if iTerm2 is available
        Command::new("osascript")
            .args(&["-e", "tell application \"iTerm\" to get version"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

#[cfg(target_os = "macos")]
impl ITerm2 {
    fn run_applescript(&self, script: &str) -> Result<()> {
        let output = Command::new("osascript")
            .args(&["-e", script])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to execute AppleScript: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("AppleScript failed: {}", error).into());
        }

        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub struct AppleTerminal;

#[cfg(target_os = "macos")]
impl Terminal for AppleTerminal {
    fn open_tab(&self, path: &Path, _session_id: Option<&str>) -> Result<()> {
        let script = format!(
            r#"
tell application "Terminal"
    tell window 1
        do script "cd '{}'" in (make new tab)
    end tell
end tell
"#,
            path.display()
        );

        self.run_applescript(&script)
    }

    fn open_window(&self, path: &Path, _session_id: Option<&str>) -> Result<()> {
        let script = format!(
            r#"
tell application "Terminal"
    do script "cd '{}'"
end tell
"#,
            path.display()
        );

        self.run_applescript(&script)
    }

    fn switch_to_directory(&self, path: &Path) -> Result<()> {
        println!("cd '{}'", path.display());
        Ok(())
    }

    fn echo_commands(&self, path: &Path) -> Result<()> {
        println!("# Navigate to worktree:");
        println!("cd '{}'", path.display());
        Ok(())
    }

    fn is_supported(&self) -> bool {
        true // Terminal.app is always available on macOS
    }
}

#[cfg(target_os = "macos")]
impl AppleTerminal {
    fn run_applescript(&self, script: &str) -> Result<()> {
        let output = Command::new("osascript")
            .args(&["-e", script])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to execute AppleScript: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("AppleScript failed: {}", error).into());
        }

        Ok(())
    }
}

pub struct TerminalManager;

impl TerminalManager {
    pub fn get_default_terminal() -> Result<Box<dyn Terminal>> {
        #[cfg(target_os = "macos")]
        {
            let iterm2 = ITerm2;
            if iterm2.is_supported() {
                Ok(Box::new(iterm2))
            } else {
                Ok(Box::new(AppleTerminal))
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(GitWarpError::TerminalNotSupported.into())
        }
    }

    pub fn switch_to_worktree<P: AsRef<Path>>(
        &self,
        path: P,
        mode: TerminalMode,
        session_id: Option<&str>,
    ) -> Result<()> {
        let terminal = Self::get_default_terminal()?;
        let path = path.as_ref();

        match mode {
            TerminalMode::Tab => terminal.open_tab(path, session_id),
            TerminalMode::Window => terminal.open_window(path, session_id),
            TerminalMode::InPlace => terminal.switch_to_directory(path),
            TerminalMode::Echo => terminal.echo_commands(path),
        }
    }
}
