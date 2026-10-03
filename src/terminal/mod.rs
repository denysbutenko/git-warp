use crate::error::{GitWarpError, Result};
use std::path::Path;

mod shell;
mod types;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(unix)]
mod unix;

#[cfg(any(windows, test))]
mod windows;

pub use types::{
    Terminal, TerminalLaunchOptions, TerminalMode, TerminalPreference, resolve_terminal_preference,
    validate_terminal_app,
};

#[cfg(target_os = "macos")]
pub use macos::{AppleTerminal, ITerm2, WarpTerminal};

use shell::print_shell_commands;
use types::parse_terminal_preference;

#[cfg(unix)]
use unix::enter_current_shell;
#[cfg(windows)]
use windows::enter_current_shell;

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
            let _ = parse_terminal_preference("auto");
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
}
