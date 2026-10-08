use super::types::TerminalLaunchOptions;
use crate::error::{GitWarpError, Result};
use std::path::Path;

#[cfg(target_os = "macos")]
pub(super) fn percent_encode(input: &str) -> String {
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

pub(super) fn shell_quote(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\\''"))
}

pub(super) fn replace_placeholders(command: &str, branch: &str, repo: &str, path: &str) -> String {
    command
        .replace("{{branch}}", branch)
        .replace("{{repo}}", repo)
        .replace("{{path}}", path)
}

/// Reject a resolved terminal command containing control characters that
/// would break out of the target syntax (AppleScript string literal on
/// macOS, single-line `/K` argument on cmd.exe, `-Command` on PowerShell).
/// Tab is allowed; NUL / CR / LF are always rejected regardless of source.
pub(super) fn ensure_command_is_single_line(command: &str) -> Result<()> {
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

pub(super) fn shell_command_sequence(
    path: &Path,
    options: &TerminalLaunchOptions,
) -> Result<Vec<String>> {
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

pub(super) fn print_shell_commands(path: &Path, options: &TerminalLaunchOptions) -> Result<()> {
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
pub(super) fn escape_applescript_string(input: &str) -> String {
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

pub(super) fn build_init_lines(
    options: &TerminalLaunchOptions,
    path: &Path,
) -> Result<Vec<String>> {
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
}
