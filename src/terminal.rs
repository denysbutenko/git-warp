use crate::error::{GitWarpError, Result};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy)]
pub enum TerminalMode {
    Tab,
    Window,
    InPlace,
    Echo,
    Current,
}

impl TerminalMode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "tab" => Some(Self::Tab),
            "window" => Some(Self::Window),
            "inplace" => Some(Self::InPlace),
            "echo" => Some(Self::Echo),
            "current" => Some(Self::Current),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TerminalLaunchOptions {
    pub auto_activate: bool,
    pub init_commands: Vec<String>,
}

impl Default for TerminalLaunchOptions {
    fn default() -> Self {
        Self {
            auto_activate: true,
            init_commands: Vec::new(),
        }
    }
}

pub trait Terminal {
    fn open_tab(
        &self,
        path: &Path,
        session_id: Option<&str>,
        options: &TerminalLaunchOptions,
    ) -> Result<()>;
    fn open_window(
        &self,
        path: &Path,
        session_id: Option<&str>,
        options: &TerminalLaunchOptions,
    ) -> Result<()>;
    fn switch_to_directory(&self, path: &Path, options: &TerminalLaunchOptions) -> Result<()>;
    fn echo_commands(&self, path: &Path, options: &TerminalLaunchOptions) -> Result<()>;
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
    fn open_tab(
        &self,
        path: &Path,
        _session_id: Option<&str>,
        options: &TerminalLaunchOptions,
    ) -> Result<()> {
        let activate = applescript_activate(options);
        let commands = iterm_write_commands(path, options, "                ");
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
        let commands = iterm_write_commands(path, options, "                ");
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

    fn switch_to_directory(&self, path: &Path, options: &TerminalLaunchOptions) -> Result<()> {
        print_shell_commands(path, options);
        Ok(())
    }

    fn echo_commands(&self, path: &Path, options: &TerminalLaunchOptions) -> Result<()> {
        println!("# Navigate to worktree:");
        print_shell_commands(path, options);
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
    fn open_tab(
        &self,
        path: &Path,
        _session_id: Option<&str>,
        options: &TerminalLaunchOptions,
    ) -> Result<()> {
        let activate = applescript_activate(options);
        let commands = terminal_tab_commands(path, options, "        ");
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
        let commands = terminal_window_commands(path, options, "    ");
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

    fn switch_to_directory(&self, path: &Path, options: &TerminalLaunchOptions) -> Result<()> {
        print_shell_commands(path, options);
        Ok(())
    }

    fn echo_commands(&self, path: &Path, options: &TerminalLaunchOptions) -> Result<()> {
        println!("# Navigate to worktree:");
        print_shell_commands(path, options);
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

    fn switch_to_directory(&self, path: &Path, options: &TerminalLaunchOptions) -> Result<()> {
        print_shell_commands(path, options);
        Ok(())
    }

    fn echo_commands(&self, path: &Path, options: &TerminalLaunchOptions) -> Result<()> {
        println!("# Navigate to worktree:");
        print_shell_commands(path, options);
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

fn shell_quote(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\\''"))
}

fn shell_command_sequence(path: &Path, options: &TerminalLaunchOptions) -> Vec<String> {
    let mut commands = vec![format!("cd {}", shell_quote(&path.to_string_lossy()))];
    commands.extend(
        options
            .init_commands
            .iter()
            .map(|command| command.trim())
            .filter(|command| !command.is_empty())
            .map(ToString::to_string),
    );
    commands
}

fn print_shell_commands(path: &Path, options: &TerminalLaunchOptions) {
    for command in shell_command_sequence(path, options) {
        println!("{command}");
    }
}

fn escape_applescript_string(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(target_os = "macos")]
fn applescript_activate(options: &TerminalLaunchOptions) -> &'static str {
    if options.auto_activate {
        "    activate"
    } else {
        ""
    }
}

#[cfg(target_os = "macos")]
fn iterm_write_commands(path: &Path, options: &TerminalLaunchOptions, indent: &str) -> String {
    shell_command_sequence(path, options)
        .into_iter()
        .map(|command| {
            format!(
                "{indent}write text \"{}\"",
                escape_applescript_string(&command)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(target_os = "macos")]
fn terminal_tab_commands(path: &Path, options: &TerminalLaunchOptions, indent: &str) -> String {
    let commands = shell_command_sequence(path, options);
    let Some((first, rest)) = commands.split_first() else {
        return String::new();
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
    lines.join("\n")
}

#[cfg(target_os = "macos")]
fn terminal_window_commands(path: &Path, options: &TerminalLaunchOptions, indent: &str) -> String {
    let commands = shell_command_sequence(path, options);
    let Some((first, rest)) = commands.split_first() else {
        return String::new();
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
    lines.join("\n")
}

fn enter_current_shell(path: &Path, options: &TerminalLaunchOptions) -> Result<()> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "/bin/sh".to_string());

    println!(
        "🐚 Starting shell in current terminal at: {}",
        path.display()
    );

    let mut command = Command::new(&shell);
    command.current_dir(path);

    if !options.init_commands.is_empty() {
        let mut init_script = options.init_commands.join("\n");
        init_script.push_str(&format!("\nexec {}", shell_quote(&shell)));
        command.args(["-lc", &init_script]);
    }

    let status = command
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to start current terminal shell: {}", e))?;

    if !status.success() {
        return Err(
            anyhow::anyhow!("Current terminal shell exited with status: {}", status).into(),
        );
    }

    Ok(())
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
        self.switch_to_worktree_with_options(
            path,
            mode,
            session_id,
            preferred_app,
            &TerminalLaunchOptions::default(),
        )
    }

    pub fn switch_to_worktree_with_options<P: AsRef<Path>>(
        &self,
        path: P,
        mode: TerminalMode,
        session_id: Option<&str>,
        preferred_app: Option<&str>,
        options: &TerminalLaunchOptions,
    ) -> Result<()> {
        let path = path.as_ref();

        if matches!(mode, TerminalMode::Current) {
            return enter_current_shell(path, options);
        }

        let terminal = Self::get_terminal(preferred_app)?;

        match mode {
            TerminalMode::Tab => terminal.open_tab(path, session_id, options),
            TerminalMode::Window => terminal.open_window(path, session_id, options),
            TerminalMode::InPlace => terminal.switch_to_directory(path, options),
            TerminalMode::Echo => terminal.echo_commands(path, options),
            TerminalMode::Current => unreachable!("current mode is handled before terminal lookup"),
        }
    }
}
