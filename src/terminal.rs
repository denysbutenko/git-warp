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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalPreference {
    ITerm2,
    AppleTerminal,
    Warp,
}

fn parse_terminal_preference(value: &str) -> Option<TerminalPreference> {
    match value.to_lowercase().as_str() {
        "iterm" | "iterm2" => Some(TerminalPreference::ITerm2),
        "terminal" => Some(TerminalPreference::AppleTerminal),
        "warp" => Some(TerminalPreference::Warp),
        _ => None,
    }
}

pub fn resolve_terminal_preference(
    preferred_app: &str,
    term_program: Option<&str>,
    iterm_supported: bool,
    warp_supported: bool,
) -> TerminalPreference {
    if let Some(explicit) = parse_terminal_preference(preferred_app) {
        return explicit;
    }

    match term_program {
        Some("WarpTerminal") if warp_supported => TerminalPreference::Warp,
        Some("iTerm.app") if iterm_supported => TerminalPreference::ITerm2,
        Some("Apple_Terminal") => TerminalPreference::AppleTerminal,
        _ if iterm_supported => TerminalPreference::ITerm2,
        _ if warp_supported => TerminalPreference::Warp,
        _ => TerminalPreference::AppleTerminal,
    }
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

#[cfg(target_os = "macos")]
pub struct WarpTerminal;

#[cfg(target_os = "macos")]
impl Terminal for WarpTerminal {
    fn open_tab(&self, path: &Path, _session_id: Option<&str>) -> Result<()> {
        self.open_uri("new_tab", path)
    }

    fn open_window(&self, path: &Path, _session_id: Option<&str>) -> Result<()> {
        self.open_uri("new_window", path)
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
        Command::new("osascript")
            .args(&["-e", "tell application \"Warp\" to get version"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

#[cfg(target_os = "macos")]
impl WarpTerminal {
    fn open_uri(&self, action: &str, path: &Path) -> Result<()> {
        let encoded_path = percent_encode(path.to_string_lossy().as_ref());
        let uri = format!("warp://action/{action}?path={encoded_path}");

        let output = Command::new("open")
            .arg(&uri)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to open Warp URI: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Warp URI open failed: {}", error).into());
        }

        Ok(())
    }
}

fn percent_encode(input: &str) -> String {
    let mut encoded = String::new();

    for byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{:02X}", byte)),
        }
    }

    encoded
}

pub struct TerminalManager;

impl TerminalManager {
    pub fn get_default_terminal() -> Result<Box<dyn Terminal>> {
        Self::get_terminal(None)
    }

    pub fn get_terminal(preferred_app: Option<&str>) -> Result<Box<dyn Terminal>> {
        #[cfg(target_os = "macos")]
        {
            let iterm2 = ITerm2;
            let warp = WarpTerminal;
            let requested = preferred_app.unwrap_or("auto");

            if matches!(
                parse_terminal_preference(requested),
                Some(TerminalPreference::ITerm2)
            ) && !iterm2.is_supported()
            {
                return Err(GitWarpError::TerminalNotSupported.into());
            }

            if matches!(
                parse_terminal_preference(requested),
                Some(TerminalPreference::Warp)
            ) && !warp.is_supported()
            {
                return Err(GitWarpError::TerminalNotSupported.into());
            }

            let term_program = std::env::var("TERM_PROGRAM").ok();
            let resolved = resolve_terminal_preference(
                requested,
                term_program.as_deref(),
                iterm2.is_supported(),
                warp.is_supported(),
            );

            match resolved {
                TerminalPreference::ITerm2 => Ok(Box::new(ITerm2)),
                TerminalPreference::AppleTerminal => Ok(Box::new(AppleTerminal)),
                TerminalPreference::Warp => Ok(Box::new(WarpTerminal)),
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
        self.switch_to_worktree_with_app(path, mode, session_id, None)
    }

    pub fn switch_to_worktree_with_app<P: AsRef<Path>>(
        &self,
        path: P,
        mode: TerminalMode,
        session_id: Option<&str>,
        preferred_app: Option<&str>,
    ) -> Result<()> {
        let terminal = Self::get_terminal(preferred_app)?;
        let path = path.as_ref();

        match mode {
            TerminalMode::Tab => terminal.open_tab(path, session_id),
            TerminalMode::Window => terminal.open_window(path, session_id),
            TerminalMode::InPlace => terminal.switch_to_directory(path),
            TerminalMode::Echo => terminal.echo_commands(path),
        }
    }
}
