use crate::integration::cli_surface_support::{
    expected_config_path, normalized_path_text, run_git, setup_test_repo, warp_command,
    write_fake_editor,
};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

const PUBLIC_SUBCOMMAND_NAMES: &[&str] = &[
    "switch",
    "ls",
    "list",
    "cleanup",
    "config",
    "agents",
    "doctor",
    "release-check",
    "hooks-install",
    "hooks-remove",
    "hooks-status",
    "shell-config",
];

#[test]
fn test_config_edit_creates_config_and_launches_editor() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    let home_dir = tempdir().unwrap();
    let config_path = expected_config_path(home_dir.path());
    let marker_path = home_dir.path().join("editor-marker.txt");
    let editor_path = write_fake_editor(&home_dir.path().join("fake-editor"), &marker_path);

    let mut command = warp_command(repo_path);
    command
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .env("EDITOR", &editor_path)
        .env_remove("VISUAL");
    let output = command.args(["config", "--edit"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(config_path.exists());
    assert_eq!(
        normalized_path_text(fs::read_to_string(&marker_path).unwrap().trim()),
        normalized_path_text(config_path.display().to_string())
    );
}

#[test]
fn test_shell_config_bash_outputs_reusable_function() {
    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["shell-config", "bash"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("warp_cd()"));
    assert!(stdout.contains("warp --terminal echo"));
    assert!(stdout.contains("complete -F _warp_completion warp"));
    assert!(stdout.contains("warp __complete branches \"$cur\""));
}

#[test]
fn test_complete_branches_outputs_local_branches_matching_prefix() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    run_git(repo_path, &["branch", "261-autocomplete"]);
    run_git(repo_path, &["branch", "261-other"]);
    run_git(repo_path, &["branch", "feature/261-nested"]);

    let output = warp_command(repo_path)
        .args(["__complete", "branches", "261"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("261-autocomplete"));
    assert!(stdout.contains("261-other"));
    assert!(!stdout.contains("feature/261-nested"));
    assert!(!stdout.contains("main"));
}

#[test]
fn test_complete_branches_includes_remote_tracking_tags_and_dedupes() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    run_git(repo_path, &["branch", "254-local"]);
    run_git(repo_path, &["tag", "254-tag"]);
    run_git(
        repo_path,
        &["update-ref", "refs/remotes/origin/254-remote-only", "HEAD"],
    );
    run_git(repo_path, &["branch", "254-dup"]);
    run_git(
        repo_path,
        &["update-ref", "refs/remotes/origin/254-dup", "HEAD"],
    );
    run_git(
        repo_path,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/254-remote-only",
        ],
    );

    let output = warp_command(repo_path)
        .args(["__complete", "branches", "254"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");

    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.contains(&"254-local"), "{stdout}");
    assert!(lines.contains(&"254-tag"), "{stdout}");
    assert!(lines.contains(&"254-remote-only"), "{stdout}");
    assert!(lines.contains(&"254-dup"), "{stdout}");
    assert_eq!(
        lines.iter().filter(|l| **l == "254-dup").count(),
        1,
        "expected 254-dup exactly once, got {stdout}"
    );
    assert!(!lines.contains(&"HEAD"), "{stdout}");
    assert!(!lines.contains(&"origin/254-remote-only"), "{stdout}");
}

#[test]
fn test_shell_config_zsh_outputs_branch_completion_for_root_and_switch() {
    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["shell-config", "zsh"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("warp_cd()"));
    assert!(stdout.contains("compdef _warp_completion warp"));
    assert!(stdout.contains("warp __complete branches \"$PREFIX\""));
    assert!(stdout.contains("CURRENT == 2"));
    assert!(stdout.contains("${words[2]} == switch && $CURRENT == 3"));
}

#[test]
fn test_shell_config_fish_outputs_branch_completion_for_root_and_switch() {
    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["shell-config", "fish"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("function warp_cd"));
    assert!(stdout.contains("__fish_use_subcommand"));
    assert!(stdout.contains("__fish_seen_subcommand_from switch"));
    assert!(stdout.contains("warp __complete branches (commandline -ct)"));
}

#[test]
fn test_shell_config_powershell_outputs_reusable_function() {
    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["shell-config", "powershell"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("function warp_cd"));
    assert!(stdout.contains("warp --terminal echo @args"));
    assert!(stdout.contains("Invoke-Expression"));
    assert!(stdout.contains("Register-ArgumentCompleter -CommandName warp -Native"));
    assert!(stdout.contains("warp __complete branches $wordToComplete"));
    assert!(stdout.contains("$commandAst.CommandElements"));
}

#[test]
fn test_shell_config_pwsh_alias_matches_powershell() {
    let ps = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["shell-config", "powershell"])
        .output()
        .unwrap();
    let pwsh = Command::new(env!("CARGO_BIN_EXE_warp"))
        .args(["shell-config", "pwsh"])
        .output()
        .unwrap();

    assert!(ps.status.success());
    assert!(pwsh.status.success());
    assert_eq!(ps.stdout, pwsh.stdout);
}

#[test]
fn test_shell_config_lists_every_public_subcommand() {
    for shell in ["bash", "zsh", "fish", "powershell"] {
        let output = Command::new(env!("CARGO_BIN_EXE_warp"))
            .args(["shell-config", shell])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "shell-config {shell} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        for name in PUBLIC_SUBCOMMAND_NAMES {
            assert!(
                stdout.contains(name),
                "shell-config {shell} snippet is missing subcommand `{name}`:\n{stdout}"
            );
        }
    }
}
