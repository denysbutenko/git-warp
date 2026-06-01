use crate::error::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal as RatatuiTerminal, backend::CrosstermBackend};
use std::io;

/// Guard that handles terminal initialization and restoration.
/// Entering raw mode and alternate screen on creation, and restoring
/// the terminal on drop or explicit restore.
pub struct TuiTerminalGuard {
    active: bool,
}

impl TuiTerminalGuard {
    pub fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(err) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture) {
            return Err(rollback_terminal_entry(
                err.into(),
                disable_raw_mode,
                || execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture),
            ));
        }
        Ok(Self { active: true })
    }

    pub fn restore(&mut self) -> Result<()> {
        let (active, result) = terminal_cleanup_attempt(self.active, disable_raw_mode, || {
            execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)
        });
        self.active = active;
        result
    }
}

impl Drop for TuiTerminalGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

/// Combines multiple errors into a single Error, preserving the original error
/// message as the primary and listing follow-on failures.
pub fn combine_errors(
    primary: anyhow::Error,
    additional: impl IntoIterator<Item = anyhow::Error>,
) -> anyhow::Error {
    let mut message = primary.to_string();
    for err in additional {
        message.push_str("; ");
        message.push_str(&err.to_string());
    }

    anyhow::anyhow!(message)
}

fn terminal_cleanup_attempt<FDisable, FCleanup>(
    active: bool,
    disable_raw: FDisable,
    cleanup_terminal: FCleanup,
) -> (bool, Result<()>)
where
    FDisable: FnOnce() -> io::Result<()>,
    FCleanup: FnOnce() -> io::Result<()>,
{
    if !active {
        return (false, Ok(()));
    }

    let mut errors = Vec::new();

    if let Err(err) = disable_raw() {
        errors.push(anyhow::Error::new(err));
    }

    if let Err(err) = cleanup_terminal() {
        errors.push(anyhow::Error::new(err));
    }

    if errors.is_empty() {
        (false, Ok(()))
    } else {
        let first = errors.remove(0);
        (true, Err(combine_errors(first, errors)))
    }
}

fn rollback_terminal_entry<FDisable, FCleanup>(
    primary: anyhow::Error,
    disable_raw: FDisable,
    cleanup_terminal: FCleanup,
) -> anyhow::Error
where
    FDisable: FnOnce() -> io::Result<()>,
    FCleanup: FnOnce() -> io::Result<()>,
{
    let (_, rollback_result) = terminal_cleanup_attempt(true, disable_raw, cleanup_terminal);
    match rollback_result {
        Ok(()) => primary,
        Err(rollback_error) => combine_errors(primary, [rollback_error]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_cleanup_attempt_keeps_guard_active_on_failure() {
        let (active, result) =
            terminal_cleanup_attempt(true, || Err(io::Error::other("disable failed")), || Ok(()));

        assert!(active);
        let message = result.expect_err("cleanup should fail").to_string();
        assert!(message.contains("disable failed"));
    }

    #[test]
    fn test_terminal_cleanup_attempt_keeps_guard_active_when_cleanup_fails() {
        let (active, result) =
            terminal_cleanup_attempt(true, || Ok(()), || Err(io::Error::other("cleanup failed")));

        assert!(active);
        let message = result.expect_err("cleanup should fail").to_string();
        assert!(message.contains("cleanup failed"));
    }

    #[test]
    fn test_terminal_cleanup_attempt_deactivates_guard_on_success() {
        let (active, result) = terminal_cleanup_attempt(true, || Ok(()), || Ok(()));

        assert!(!active);
        assert!(result.is_ok());
    }

    #[test]
    fn test_rollback_terminal_entry_combines_primary_and_cleanup_failures() {
        let error = rollback_terminal_entry(
            anyhow::anyhow!("enter failed"),
            || Err(io::Error::other("disable failed")),
            || Err(io::Error::other("leave failed")),
        );

        let message = error.to_string();
        assert!(message.contains("enter failed"));
        assert!(message.contains("disable failed"));
        assert!(message.contains("leave failed"));
    }
}
