use crate::error::{GitWarpError, Result};

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
        path: &std::path::Path,
        session_id: Option<&str>,
        options: &TerminalLaunchOptions,
    ) -> Result<()>;
    fn open_window(
        &self,
        path: &std::path::Path,
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

pub(super) fn parse_terminal_preference(value: &str) -> Option<TerminalPreference> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
