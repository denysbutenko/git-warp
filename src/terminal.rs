use crate::error::{GitWarpError, Result};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum TerminalMode {
    Tab,
    Window,
    #[value(name = "inplace")]
    InPlace,
    Echo,
    Current,
}

impl TerminalMode {
    pub const SUPPORTED: &'static [&'static str] = &["tab", "window", "inplace", "echo", "current"];

    // Returns Option, not Result — we deliberately don't implement std::str::FromStr.
    #[allow(clippy::should_implement_trait)]
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
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub auto_activate: bool,
    pub init_commands: Vec<String>,
    pub branch: Option<String>,
    pub repo: Option<String>,
}

impl Default for TerminalLaunchOptions {
    fn default() -> Self {
        Self {
            auto_activate: true,
            init_commands: Vec::new(),
            branch: None,
            repo: None,
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
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn is_supported(&self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub enum TerminalPreference {
    Auto,
    ITerm2,
    AppleTerminal,
    Warp,
}

impl TerminalPreference {
    pub const SUPPORTED: &'static [&'static str] = &["auto", "iterm2", "terminal", "warp"];
}

fn parse_terminal_preference(value: &str) -> Option<TerminalPreference> {
    match value.to_lowercase().as_str() {
        "auto" => Some(TerminalPreference::Auto),
        "iterm" | "iterm2" => Some(TerminalPreference::ITerm2),
        "terminal" => Some(TerminalPreference::AppleTerminal),
        "warp" => Some(TerminalPreference::Warp),
        _ => None,
    }
}

/// Reject unknown `terminal.app` values up front instead of falling back to
/// whatever `TERM_PROGRAM` happens to be set to.
pub fn validate_terminal_app(value: &str) -> Result<()> {
    if parse_terminal_preference(value).is_some() {
        return Ok(());
    }
    Err(GitWarpError::ConfigError {
        message: format!(
            "Invalid terminal app '{}'. Supported apps: {}",
            value,
            TerminalPreference::SUPPORTED.join(", "),
        ),
    }
    .into())
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn resolve_terminal_preference(
    preferred_app: &str,
    term_program: Option<&str>,
    iterm_supported: bool,
    warp_supported: bool,
) -> TerminalPreference {
    match parse_terminal_preference(preferred_app) {
        Some(TerminalPreference::Auto) | None => {}
        Some(explicit) => return explicit,
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

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
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

    fn is_supported(&self) -> bool {
        Command::new("osascript")
            .args(["-e", "tell application \"Warp\" to get version"])
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
            return Err(anyhow::anyhow!("Warp URI open failed: {}", error));
        }

        Ok(())
    }
}

#[cfg(target_os = "macos")]
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

fn replace_placeholders(command: &str, branch: &str, repo: &str, path: &str) -> String {
    command
        .replace("{{branch}}", branch)
        .replace("{{repo}}", repo)
        .replace("{{path}}", path)
}

/// Reject a resolved terminal command containing control characters that
/// would break out of the target syntax (AppleScript string literal on
/// macOS, single-line `/K` argument on cmd.exe, `-Command` on PowerShell).
/// Tab is allowed; NUL / CR / LF are always rejected regardless of source.
fn ensure_command_is_single_line(command: &str) -> Result<()> {
    if let Some(ch) = command.chars().find(|ch| matches!(*ch, '\0' | '\n' | '\r')) {
        return Err(GitWarpError::ConfigError {
            message: format!(
                "terminal command contains a disallowed control character \
                 (U+{:04X}); check `[terminal].init_commands` and any \
                 substituted branch/repo/path values: {command:?}",
                ch as u32,
            ),
        }
        .into());
    }
    Ok(())
}

fn shell_command_sequence(path: &Path, options: &TerminalLaunchOptions) -> Result<Vec<String>> {
    let path_str = path.to_string_lossy();
    let cd = format!("cd {}", shell_quote(&path_str));
    ensure_command_is_single_line(&cd)?;

    let mut commands = vec![cd];
    let branch = options.branch.as_deref().unwrap_or("");
    let repo = options.repo.as_deref().unwrap_or("");

    for command in options
        .init_commands
        .iter()
        .map(|command| command.trim())
        .filter(|command| !command.is_empty())
    {
        let resolved = replace_placeholders(command, branch, repo, &path_str);
        ensure_command_is_single_line(&resolved)?;
        commands.push(resolved);
    }
    Ok(commands)
}

fn print_shell_commands(path: &Path, options: &TerminalLaunchOptions) -> Result<()> {
    for command in shell_command_sequence(path, options)? {
        println!("{command}");
    }
    Ok(())
}

/// Escape a string for embedding inside an AppleScript double-quoted string
/// literal. Callers must additionally run each command through
/// [`ensure_command_is_single_line`] so that a literal newline in user input
/// cannot terminate the AppleScript source line early.
#[cfg(any(target_os = "macos", test))]
fn escape_applescript_string(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
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

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
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

fn build_init_lines(options: &TerminalLaunchOptions, path: &Path) -> Result<Vec<String>> {
    let branch = options.branch.as_deref().unwrap_or("");
    let repo = options.repo.as_deref().unwrap_or("");
    let path_str = path.to_string_lossy();
    options
        .init_commands
        .iter()
        .map(|command| {
            let resolved = replace_placeholders(command, branch, repo, &path_str);
            ensure_command_is_single_line(&resolved)?;
            Ok(resolved)
        })
        .collect()
}

#[cfg(unix)]
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

    let lines = build_init_lines(options, path)?;
    if !lines.is_empty() {
        let mut init_script = lines.join("\n");
        init_script.push_str(&format!("\nexec {}", shell_quote(&shell)));
        command.args(["-lc", &init_script]);
    }

    let status = command.status().map_err(|e| {
        anyhow::anyhow!("Failed to start current terminal shell '{}': {}", shell, e)
    })?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "Current terminal shell exited with status: {}",
            status
        ));
    }

    Ok(())
}

#[cfg(windows)]
fn enter_current_shell(path: &Path, options: &TerminalLaunchOptions) -> Result<()> {
    println!(
        "🐚 Starting shell in current terminal at: {}",
        path.display()
    );

    let shell_env = std::env::var("SHELL").ok();
    let ps_module_path = std::env::var("PSModulePath").ok();
    let comspec = std::env::var("ComSpec").ok();
    let path_env = std::env::var_os("PATH");

    let lines = build_init_lines(options, path)?;
    let spec = resolve_windows_current_shell(
        shell_env.as_deref(),
        ps_module_path.as_deref(),
        comspec.as_deref(),
        path_env.as_deref(),
        &lines,
    );

    let status = Command::new(&spec.program)
        .args(&spec.args)
        .current_dir(path)
        .status()
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to start current terminal shell '{}': {}",
                spec.program.display(),
                e
            )
        })?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "Current terminal shell exited with status: {}",
            status
        ));
    }

    Ok(())
}

#[cfg(any(windows, test))]
#[derive(Debug)]
struct WindowsShellSpec {
    program: std::path::PathBuf,
    args: Vec<std::ffi::OsString>,
}

#[cfg(any(windows, test))]
fn find_on_path_with(env_path: Option<&std::ffi::OsStr>, name: &str) -> Option<std::path::PathBuf> {
    let path = env_path?;
    for dir in std::env::split_paths(path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(any(windows, test))]
fn resolve_windows_current_shell(
    shell: Option<&str>,
    ps_module_path: Option<&str>,
    comspec: Option<&str>,
    env_path: Option<&std::ffi::OsStr>,
    init_lines: &[String],
) -> WindowsShellSpec {
    use std::ffi::OsString;
    use std::path::PathBuf;

    if let Some(shell) = shell.map(str::trim).filter(|s| !s.is_empty()) {
        let mut args: Vec<OsString> = Vec::new();
        if !init_lines.is_empty() {
            let mut script = init_lines.join("\n");
            script.push_str(&format!("\nexec {}", shell_quote(shell)));
            args.push(OsString::from("-lc"));
            args.push(OsString::from(script));
        }
        return WindowsShellSpec {
            program: PathBuf::from(shell),
            args,
        };
    }

    if ps_module_path
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        let program = find_on_path_with(env_path, "pwsh.exe")
            .or_else(|| find_on_path_with(env_path, "powershell.exe"))
            .unwrap_or_else(|| PathBuf::from("pwsh.exe"));

        let mut args: Vec<OsString> = vec![OsString::from("-NoExit")];
        if !init_lines.is_empty() {
            args.push(OsString::from("-Command"));
            args.push(OsString::from(init_lines.join("; ")));
        }
        return WindowsShellSpec { program, args };
    }

    let program = comspec
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("cmd.exe"));

    let mut args: Vec<OsString> = Vec::new();
    if !init_lines.is_empty() {
        args.push(OsString::from("/d"));
        args.push(OsString::from("/k"));
        args.push(OsString::from(init_lines.join(" & ")));
    }

    WindowsShellSpec { program, args }
}

pub struct TerminalManager;

impl TerminalManager {
    #[allow(dead_code)] // Public helper used by tests/embedders.
    pub fn get_default_terminal() -> Result<Box<dyn Terminal>> {
        Self::get_terminal(None)
    }

    pub fn get_terminal(preferred_app: Option<&str>) -> Result<Box<dyn Terminal>> {
        if let Some(value) = preferred_app {
            validate_terminal_app(value)?;
        }

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
                TerminalPreference::Auto => unreachable!(
                    "resolve_terminal_preference must collapse Auto into a concrete terminal"
                ),
                TerminalPreference::ITerm2 => Ok(Box::new(ITerm2)),
                TerminalPreference::AppleTerminal => Ok(Box::new(AppleTerminal)),
                TerminalPreference::Warp => Ok(Box::new(WarpTerminal)),
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = preferred_app;
            Err(GitWarpError::TerminalNotSupported.into())
        }
    }

    #[allow(dead_code)] // Public convenience wrapper kept for embedders.
    pub fn switch_to_worktree<P: AsRef<Path>>(
        &self,
        path: P,
        mode: TerminalMode,
        session_id: Option<&str>,
    ) -> Result<()> {
        self.switch_to_worktree_with_app(path, mode, session_id, None)
    }

    #[allow(dead_code)] // Public convenience wrapper kept for embedders.
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

        match mode {
            TerminalMode::Current => return enter_current_shell(path, options),
            TerminalMode::InPlace => {
                print_shell_commands(path, options)?;
                return Ok(());
            }
            TerminalMode::Echo => {
                println!("# Navigate to worktree:");
                print_shell_commands(path, options)?;
                return Ok(());
            }
            TerminalMode::Tab | TerminalMode::Window => {}
        }

        let terminal = Self::get_terminal(preferred_app)?;

        match mode {
            TerminalMode::Tab => terminal.open_tab(path, session_id, options),
            TerminalMode::Window => terminal.open_window(path, session_id, options),
            TerminalMode::InPlace | TerminalMode::Echo | TerminalMode::Current => {
                unreachable!("stdout-only modes are handled before terminal lookup")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_placeholders() {
        let command = "echo branch: {{branch}}, repo: {{repo}}, path: {{path}}";
        let branch = "feature/test";
        let repo = "git-warp";
        let path = "/tmp/git-warp/feature-test";

        let replaced = replace_placeholders(command, branch, repo, path);
        assert_eq!(
            replaced,
            "echo branch: feature/test, repo: git-warp, path: /tmp/git-warp/feature-test"
        );
    }

    #[test]
    fn test_replace_placeholders_no_match() {
        let command = "ls -la";
        let branch = "main";
        let repo = "warp";
        let path = "/work";

        let replaced = replace_placeholders(command, branch, repo, path);
        assert_eq!(replaced, "ls -la");
    }

    #[test]
    fn shell_command_sequence_emits_cd_and_init_commands() {
        let options = TerminalLaunchOptions {
            init_commands: vec![
                "echo branch={{branch}}".to_string(),
                "  ".to_string(),
                "ls {{path}}".to_string(),
            ],
            branch: Some("feature/x".to_string()),
            repo: Some("git-warp".to_string()),
            ..TerminalLaunchOptions::default()
        };
        let path = Path::new("/tmp/git-warp/feature-x");

        let commands = shell_command_sequence(path, &options).expect("commands resolve");

        assert_eq!(
            commands,
            vec![
                "cd '/tmp/git-warp/feature-x'".to_string(),
                "echo branch=feature/x".to_string(),
                "ls /tmp/git-warp/feature-x".to_string(),
            ]
        );
    }

    #[test]
    fn terminal_manager_inplace_succeeds_on_all_targets() {
        let path = Path::new("/tmp/git-warp/feature-x");
        let result = TerminalManager.switch_to_worktree_with_options(
            path,
            TerminalMode::InPlace,
            None,
            None,
            &TerminalLaunchOptions::default(),
        );
        assert!(result.is_ok(), "InPlace must not require a GUI terminal");
    }

    #[test]
    fn terminal_manager_echo_succeeds_on_all_targets() {
        let path = Path::new("/tmp/git-warp/feature-x");
        let result = TerminalManager.switch_to_worktree_with_options(
            path,
            TerminalMode::Echo,
            None,
            None,
            &TerminalLaunchOptions::default(),
        );
        assert!(result.is_ok(), "Echo must not require a GUI terminal");
    }

    #[test]
    fn resolve_windows_current_shell_prefers_shell_env() {
        let spec = resolve_windows_current_shell(
            Some("/usr/bin/bash"),
            Some("C:\\PSModules"),
            Some("C:\\Windows\\System32\\cmd.exe"),
            None,
            &[],
        );
        assert_eq!(spec.program, std::path::PathBuf::from("/usr/bin/bash"));
        assert!(spec.args.is_empty());
    }

    #[test]
    fn resolve_windows_current_shell_shell_env_with_init_lines() {
        let lines = vec!["echo first".to_string(), "echo second".to_string()];
        let spec = resolve_windows_current_shell(Some("/usr/bin/bash"), None, None, None, &lines);
        assert_eq!(spec.program, std::path::PathBuf::from("/usr/bin/bash"));
        assert_eq!(spec.args.len(), 2);
        assert_eq!(spec.args[0], "-lc");
        let script = spec.args[1].to_str().expect("script is utf-8");
        assert!(script.starts_with("echo first\necho second\nexec "));
        assert!(script.ends_with("exec '/usr/bin/bash'"));
    }

    #[test]
    fn resolve_windows_current_shell_prefers_pwsh_when_on_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("pwsh.exe"), b"").expect("write pwsh stub");
        std::fs::write(dir.path().join("powershell.exe"), b"").expect("write powershell stub");
        let path_env: std::ffi::OsString = dir.path().as_os_str().to_owned();

        let spec = resolve_windows_current_shell(
            None,
            Some("C:\\PSModules"),
            None,
            Some(path_env.as_os_str()),
            &[],
        );

        assert_eq!(spec.program.file_name().expect("file_name"), "pwsh.exe");
        assert_eq!(spec.args, vec![std::ffi::OsString::from("-NoExit")]);
    }

    #[test]
    fn resolve_windows_current_shell_falls_back_to_powershell() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("powershell.exe"), b"").expect("write powershell stub");
        let path_env: std::ffi::OsString = dir.path().as_os_str().to_owned();

        let spec = resolve_windows_current_shell(
            None,
            Some("C:\\PSModules"),
            None,
            Some(path_env.as_os_str()),
            &[],
        );

        assert_eq!(
            spec.program.file_name().expect("file_name"),
            "powershell.exe"
        );
    }

    #[test]
    fn resolve_windows_current_shell_pwsh_with_init_uses_no_exit_command() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("pwsh.exe"), b"").expect("write pwsh stub");
        let path_env: std::ffi::OsString = dir.path().as_os_str().to_owned();
        let lines = vec![
            "Set-Location C:\\Repo".to_string(),
            "git status".to_string(),
        ];

        let spec = resolve_windows_current_shell(
            None,
            Some("C:\\PSModules"),
            None,
            Some(path_env.as_os_str()),
            &lines,
        );

        assert_eq!(spec.args.len(), 3);
        assert_eq!(spec.args[0], "-NoExit");
        assert_eq!(spec.args[1], "-Command");
        assert_eq!(spec.args[2], "Set-Location C:\\Repo; git status");
    }

    #[test]
    fn resolve_windows_current_shell_pwsh_falls_back_to_literal_when_missing() {
        let spec = resolve_windows_current_shell(None, Some("C:\\PSModules"), None, None, &[]);
        assert_eq!(spec.program, std::path::PathBuf::from("pwsh.exe"));
        assert_eq!(spec.args, vec![std::ffi::OsString::from("-NoExit")]);
    }

    #[test]
    fn resolve_windows_current_shell_uses_comspec_when_no_shell_or_ps() {
        let lines = vec!["dir".to_string(), "echo done".to_string()];
        let spec = resolve_windows_current_shell(
            None,
            None,
            Some("C:\\Windows\\System32\\cmd.exe"),
            None,
            &lines,
        );
        assert_eq!(
            spec.program,
            std::path::PathBuf::from("C:\\Windows\\System32\\cmd.exe")
        );
        assert_eq!(spec.args.len(), 3);
        assert_eq!(spec.args[0], "/d");
        assert_eq!(spec.args[1], "/k");
        assert_eq!(spec.args[2], "dir & echo done");
    }

    #[test]
    fn resolve_windows_current_shell_defaults_to_cmd_when_comspec_blank() {
        let spec = resolve_windows_current_shell(None, None, Some("   "), None, &[]);
        assert_eq!(spec.program, std::path::PathBuf::from("cmd.exe"));
        assert!(spec.args.is_empty());
    }

    #[test]
    fn resolve_windows_current_shell_defaults_to_cmd_when_no_env_set() {
        let spec = resolve_windows_current_shell(None, None, None, None, &[]);
        assert_eq!(spec.program, std::path::PathBuf::from("cmd.exe"));
        assert!(spec.args.is_empty());
    }

    #[test]
    fn parse_terminal_preference_recognises_auto() {
        assert_eq!(
            parse_terminal_preference("auto"),
            Some(TerminalPreference::Auto)
        );
        assert_eq!(
            parse_terminal_preference("AUTO"),
            Some(TerminalPreference::Auto)
        );
    }

    #[test]
    fn parse_terminal_preference_rejects_unknown() {
        assert_eq!(parse_terminal_preference("ghostty"), None);
        assert_eq!(parse_terminal_preference("alacritty"), None);
        assert_eq!(parse_terminal_preference(""), None);
    }

    #[test]
    fn validate_terminal_app_accepts_supported_values() {
        for value in ["auto", "AUTO", "iterm", "iterm2", "terminal", "warp"] {
            assert!(
                validate_terminal_app(value).is_ok(),
                "expected '{value}' to validate"
            );
        }
    }

    #[test]
    fn validate_terminal_app_rejects_unknown_value() {
        let err = validate_terminal_app("ghostty").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("ghostty"), "message: {message}");
        for entry in TerminalPreference::SUPPORTED {
            assert!(
                message.contains(entry),
                "message {message:?} missing supported value {entry}"
            );
        }
    }

    #[test]
    fn escape_applescript_string_escapes_backslash_and_quote() {
        assert_eq!(
            escape_applescript_string("echo \"hi\" \\\\ path"),
            "echo \\\"hi\\\" \\\\\\\\ path"
        );
    }

    #[test]
    fn escape_applescript_string_escapes_control_characters() {
        assert_eq!(
            escape_applescript_string("line1\nline2\r\tend"),
            "line1\\nline2\\r\\tend"
        );
    }

    #[test]
    fn ensure_command_is_single_line_rejects_newline_carriage_return_and_nul() {
        for bad in ["echo a\necho b", "echo a\recho b", "echo\0end"] {
            let err = ensure_command_is_single_line(bad).expect_err("should reject");
            let msg = err.to_string();
            assert!(
                msg.contains("disallowed control character"),
                "unexpected error message: {msg}"
            );
        }
    }

    #[test]
    fn ensure_command_is_single_line_allows_tab_and_metacharacters() {
        // Tab is a legitimate whitespace character; cmd.exe metacharacters
        // are user-configured content that must not be blanket-rejected.
        for good in [
            "echo\tvalue",
            "echo a & echo b",
            "echo a | grep b",
            "echo %PATH%",
            "cd \"C:\\Program Files\"",
        ] {
            ensure_command_is_single_line(good).expect("should accept");
        }
    }

    #[test]
    fn shell_command_sequence_rejects_branch_with_newline() {
        let options = TerminalLaunchOptions {
            init_commands: vec!["echo branch={{branch}}".to_string()],
            branch: Some("main\nrm -rf /".to_string()),
            repo: Some("git-warp".to_string()),
            ..TerminalLaunchOptions::default()
        };
        let err = shell_command_sequence(Path::new("/tmp/x"), &options)
            .expect_err("newline in branch must be rejected");
        assert!(
            err.to_string().contains("disallowed control character"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn shell_command_sequence_rejects_init_command_with_carriage_return() {
        let options = TerminalLaunchOptions {
            init_commands: vec!["echo one\r\necho two".to_string()],
            ..TerminalLaunchOptions::default()
        };
        let err = shell_command_sequence(Path::new("/tmp/x"), &options)
            .expect_err("CRLF in init_commands must be rejected");
        assert!(
            err.to_string().contains("disallowed control character"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn build_init_lines_rejects_control_char_in_resolved_value() {
        let options = TerminalLaunchOptions {
            init_commands: vec!["echo {{repo}}".to_string()],
            repo: Some("repo\nname".to_string()),
            ..TerminalLaunchOptions::default()
        };
        let err = build_init_lines(&options, Path::new("/tmp/x"))
            .expect_err("newline in repo must be rejected");
        assert!(err.to_string().contains("disallowed control character"));
    }

    #[test]
    fn build_init_lines_preserves_cmd_metacharacters() {
        // These reach resolve_windows_current_shell today; validator must
        // not restrict user-authored shell metacharacters even though the
        // cmd.exe `&` join is a known-lossy path (out of scope for #282).
        let options = TerminalLaunchOptions {
            init_commands: vec![
                "echo one & echo two".to_string(),
                "type %USERPROFILE%\\file".to_string(),
            ],
            ..TerminalLaunchOptions::default()
        };
        let lines =
            build_init_lines(&options, Path::new("/tmp/x")).expect("metacharacters allowed");
        assert_eq!(
            lines,
            vec![
                "echo one & echo two".to_string(),
                "type %USERPROFILE%\\file".to_string(),
            ]
        );
    }

    #[test]
    fn terminal_manager_inplace_reports_error_for_newline_in_branch() {
        let options = TerminalLaunchOptions {
            init_commands: vec!["echo {{branch}}".to_string()],
            branch: Some("safe\ninjected".to_string()),
            ..TerminalLaunchOptions::default()
        };
        let err = TerminalManager
            .switch_to_worktree_with_options(
                Path::new("/tmp/git-warp/x"),
                TerminalMode::InPlace,
                None,
                None,
                &options,
            )
            .expect_err("InPlace mode must propagate the validator error");
        assert!(err.to_string().contains("disallowed control character"));
    }

    #[test]
    fn resolve_terminal_preference_auto_uses_term_program() {
        assert_eq!(
            resolve_terminal_preference("auto", Some("WarpTerminal"), false, true),
            TerminalPreference::Warp
        );
        assert_eq!(
            resolve_terminal_preference("auto", Some("Apple_Terminal"), true, true),
            TerminalPreference::AppleTerminal
        );
    }
}
