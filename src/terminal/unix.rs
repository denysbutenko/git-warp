#![cfg(unix)]

use super::shell::{build_init_lines, shell_quote};
use super::types::TerminalLaunchOptions;
use crate::error::Result;
use std::path::Path;
use std::process::Command;

pub(super) fn enter_current_shell(path: &Path, options: &TerminalLaunchOptions) -> Result<()> {
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
