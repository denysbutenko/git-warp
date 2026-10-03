#![cfg(any(windows, test))]

#[cfg(windows)]
use super::shell::build_init_lines;
use super::shell::shell_quote;
#[cfg(windows)]
use super::types::TerminalLaunchOptions;
#[cfg(windows)]
use crate::error::Result;
#[cfg(windows)]
use std::path::Path;
#[cfg(windows)]
use std::process::Command;

#[derive(Debug)]
pub(super) struct WindowsShellSpec {
    pub(super) program: std::path::PathBuf,
    pub(super) args: Vec<std::ffi::OsString>,
}

pub(super) fn find_on_path_with(
    env_path: Option<&std::ffi::OsStr>,
    name: &str,
) -> Option<std::path::PathBuf> {
    let path = env_path?;
    for dir in std::env::split_paths(path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub(super) fn resolve_windows_current_shell(
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

#[cfg(windows)]
pub(super) fn enter_current_shell(path: &Path, options: &TerminalLaunchOptions) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
