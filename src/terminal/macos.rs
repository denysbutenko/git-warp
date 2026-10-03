#![cfg(target_os = "macos")]

use super::shell::{escape_applescript_string, percent_encode, shell_command_sequence};
use super::types::{Terminal, TerminalLaunchOptions};
use crate::error::Result;
use std::path::Path;
use std::process::Command;

pub struct ITerm2;

impl Terminal for ITerm2 {
    fn open_tab(
        &self,
        path: &Path,
        _session_id: Option<&str>,
        options: &TerminalLaunchOptions,
    ) -> Result<()> {
        let activate = applescript_activate(options);
        let commands = iterm_write_commands(path, options, "                ")?;
        let script = format!(
            r#"
tell application "iTerm"
{activate}
    tell current window
        create tab with default profile
        tell current tab
            tell current session
{commands}
            end tell
        end tell
    end tell
end tell
"#,
        );

        self.run_applescript(&script)
    }

    fn open_window(
        &self,
        path: &Path,
        _session_id: Option<&str>,
        options: &TerminalLaunchOptions,
    ) -> Result<()> {
        let activate = applescript_activate(options);
        let commands = iterm_write_commands(path, options, "                ")?;
        let script = format!(
            r#"
tell application "iTerm"
{activate}
    create window with default profile
    tell current window
        tell current tab
            tell current session
{commands}
            end tell
        end tell
    end tell
end tell
"#,
        );

        self.run_applescript(&script)
    }

    fn is_supported(&self) -> bool {
        // Check if iTerm2 is available
        Command::new("osascript")
            .args(["-e", "tell application \"iTerm\" to get version"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

impl ITerm2 {
    fn run_applescript(&self, script: &str) -> Result<()> {
        let output = Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to execute AppleScript: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("AppleScript failed: {}", error));
        }

        Ok(())
    }
}

pub struct AppleTerminal;

impl Terminal for AppleTerminal {
    fn open_tab(
        &self,
        path: &Path,
        _session_id: Option<&str>,
        options: &TerminalLaunchOptions,
    ) -> Result<()> {
        let activate = applescript_activate(options);
        let commands = terminal_tab_commands(path, options, "        ")?;
        let script = format!(
            r#"
tell application "Terminal"
{activate}
    tell window 1
{commands}
    end tell
end tell
"#,
        );

        self.run_applescript(&script)
    }

    fn open_window(
        &self,
        path: &Path,
        _session_id: Option<&str>,
        options: &TerminalLaunchOptions,
    ) -> Result<()> {
        let activate = applescript_activate(options);
        let commands = terminal_window_commands(path, options, "    ")?;
        let script = format!(
            r#"
tell application "Terminal"
{activate}
{commands}
end tell
"#,
        );

        self.run_applescript(&script)
    }

    fn is_supported(&self) -> bool {
        true // Terminal.app is always available on macOS
    }
}

impl AppleTerminal {
    fn run_applescript(&self, script: &str) -> Result<()> {
        let output = Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to execute AppleScript: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("AppleScript failed: {}", error));
        }

        Ok(())
    }
}

pub struct WarpTerminal;

impl Terminal for WarpTerminal {
    fn open_tab(
        &self,
        path: &Path,
        _session_id: Option<&str>,
        _options: &TerminalLaunchOptions,
    ) -> Result<()> {
        self.open_uri("new_tab", path)
    }

    fn open_window(
        &self,
        path: &Path,
        _session_id: Option<&str>,
        _options: &TerminalLaunchOptions,
    ) -> Result<()> {
        self.open_uri("new_window", path)
    }

    fn is_supported(&self) -> bool {
        Command::new("osascript")
            .args(["-e", "tell application \"Warp\" to get version"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

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
            return Err(anyhow::anyhow!("Warp URI open failed: {}", error));
        }

        Ok(())
    }
}

fn applescript_activate(options: &TerminalLaunchOptions) -> &'static str {
    if options.auto_activate {
        "    activate"
    } else {
        ""
    }
}

fn iterm_write_commands(
    path: &Path,
    options: &TerminalLaunchOptions,
    indent: &str,
) -> Result<String> {
    Ok(shell_command_sequence(path, options)?
        .into_iter()
        .map(|command| {
            format!(
                "{indent}write text \"{}\"",
                escape_applescript_string(&command)
            )
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

fn terminal_tab_commands(
    path: &Path,
    options: &TerminalLaunchOptions,
    indent: &str,
) -> Result<String> {
    let commands = shell_command_sequence(path, options)?;
    let Some((first, rest)) = commands.split_first() else {
        return Ok(String::new());
    };

    let mut lines = vec![format!(
        "{indent}do script \"{}\" in (make new tab)",
        escape_applescript_string(first)
    )];
    lines.extend(rest.iter().map(|command| {
        format!(
            "{indent}do script \"{}\" in selected tab",
            escape_applescript_string(command)
        )
    }));
    Ok(lines.join("\n"))
}

fn terminal_window_commands(
    path: &Path,
    options: &TerminalLaunchOptions,
    indent: &str,
) -> Result<String> {
    let commands = shell_command_sequence(path, options)?;
    let Some((first, rest)) = commands.split_first() else {
        return Ok(String::new());
    };

    let mut lines = vec![format!(
        "{indent}do script \"{}\"",
        escape_applescript_string(first)
    )];
    lines.extend(rest.iter().map(|command| {
        format!(
            "{indent}do script \"{}\" in selected tab of front window",
            escape_applescript_string(command)
        )
    }));
    Ok(lines.join("\n"))
}
