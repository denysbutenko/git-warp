use anyhow::Result;
use log::info;

use crate::cli::Cli;

pub(crate) fn public_subcommand_names() -> Vec<String> {
    use clap::CommandFactory;
    let mut names = Vec::new();
    for sub in Cli::command().get_subcommands() {
        if sub.is_hide_set() {
            continue;
        }
        names.push(sub.get_name().to_string());
        for alias in sub.get_all_aliases() {
            names.push(alias.to_string());
        }
    }
    names
}

pub fn run(_cli: &Cli, shell: Option<&str>) -> Result<()> {
    info!("Generating shell config for: {:?}", shell);
    let detected_shell = shell
        .map(str::to_string)
        .or_else(|| {
            std::env::var("SHELL").ok().and_then(|value| {
                value
                    .rsplit('/')
                    .next()
                    .map(str::to_string)
                    .filter(|value| !value.is_empty())
            })
        })
        .or_else(|| {
            if cfg!(windows) && std::env::var_os("PSModulePath").is_some() {
                Some("powershell".to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "bash".to_string());

    let commands = public_subcommand_names().join(" ");

    match detected_shell.as_str() {
        "bash" => {
            println!("# Add to ~/.bashrc");
            println!("warp_cd() {{ eval \"$(warp --terminal echo \"$@\")\"; }}");
            println!(
                "_warp_completion() {{
    local cur prev commands branches
    cur=\"${{COMP_WORDS[COMP_CWORD]}}\"
    prev=\"${{COMP_WORDS[COMP_CWORD-1]}}\"
    commands=\"{commands}\"

    if [[ \"$prev\" == \"switch\" ]]; then
        branches=\"$(warp __complete branches \"$cur\" 2>/dev/null)\"
        COMPREPLY=($(compgen -W \"$branches\" -- \"$cur\"))
    elif [[ $COMP_CWORD -eq 1 ]]; then
        branches=\"$(warp __complete branches \"$cur\" 2>/dev/null)\"
        COMPREPLY=($(compgen -W \"$commands $branches\" -- \"$cur\"))
    fi
}}
complete -F _warp_completion warp"
            );
        }
        "zsh" => {
            println!("# Add to ~/.zshrc");
            println!("warp_cd() {{ eval \"$(warp --terminal echo \"$@\")\"; }}");
            println!(
                "_warp_branch_completions() {{
    local -a branches
    branches=(\"${{(@f)$(warp __complete branches \"$PREFIX\" 2>/dev/null)}}\")
    compadd -- \"${{branches[@]}}\"
}}

_warp_completion() {{
    local -a commands
    commands=({commands})

    if (( CURRENT == 2 )); then
        compadd -- \"${{commands[@]}}\"
        _warp_branch_completions
    elif [[ ${{words[2]}} == switch && $CURRENT == 3 ]]; then
        _warp_branch_completions
    fi
}}
compdef _warp_completion warp"
            );
        }
        "fish" => {
            println!("# Add to ~/.config/fish/config.fish");
            println!("function warp_cd");
            println!("    eval (warp --terminal echo $argv)");
            println!("end");
            println!(
                "complete -c warp -n '__fish_use_subcommand' -a '{commands}'
complete -c warp -n '__fish_use_subcommand' -f -a '(warp __complete branches (commandline -ct) 2>/dev/null)'
complete -c warp -n '__fish_seen_subcommand_from switch' -f -a '(warp __complete branches (commandline -ct) 2>/dev/null)'"
            );
        }
        "powershell" | "pwsh" => {
            let ps_subcommands = public_subcommand_names()
                .iter()
                .map(|name| format!("'{name}'"))
                .collect::<Vec<_>>()
                .join(",");
            println!("# Add to $PROFILE.CurrentUserAllHosts");
            println!("function warp_cd {{");
            println!("    $script = (warp --terminal echo @args) -join \"`n\"");
            println!("    if ($script) {{ Invoke-Expression $script }}");
            println!("}}");
            println!(
                "Register-ArgumentCompleter -CommandName warp -Native -ScriptBlock {{
    param($wordToComplete, $commandAst, $cursorPosition)
    $subcommands = @({ps_subcommands})
    $elements = $commandAst.CommandElements
    $subcommand = if ($elements.Count -ge 2) {{ $elements[1].Extent.Text }} else {{ '' }}
    if ($subcommand -eq 'switch' -or $elements.Count -le 2) {{
        foreach ($branch in @(warp __complete branches $wordToComplete 2>$null)) {{
            [System.Management.Automation.CompletionResult]::new($branch, $branch, 'ParameterValue', $branch)
        }}
    }}
    if ($elements.Count -le 2) {{
        foreach ($cmd in $subcommands) {{
            if ($cmd.StartsWith($wordToComplete)) {{
                [System.Management.Automation.CompletionResult]::new($cmd, $cmd, 'ParameterValue', $cmd)
            }}
        }}
    }}
}}"
            );
        }
        other => {
            return Err(anyhow::anyhow!(
                "Unsupported shell '{other}'. Supported shells: bash, zsh, fish, powershell"
            ));
        }
    }
    Ok(())
}
