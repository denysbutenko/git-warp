use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Filename of the `warp` binary on the current platform (`warp.exe` on Windows,
/// `warp` elsewhere).
pub fn warp_executable_name() -> String {
    format!("warp{}", std::env::consts::EXE_SUFFIX)
}

pub struct ReleaseCheckOptions {
    pub version: Option<String>,
    pub metadata_only: bool,
}

#[derive(Debug, PartialEq)]
pub struct ReleaseVersion {
    pub number: String,
    pub tag: String,
}

#[derive(Debug, PartialEq)]
pub struct ReleaseCheck {
    pub label: &'static str,
    pub ok: bool,
    pub detail: String,
}

impl ReleaseCheck {
    pub fn pass(label: &'static str, detail: impl Into<String>) -> Self {
        Self {
            label,
            ok: true,
            detail: detail.into(),
        }
    }

    pub fn fail(label: &'static str, detail: impl Into<String>) -> Self {
        Self {
            label,
            ok: false,
            detail: detail.into(),
        }
    }
}

pub fn run_release_check(options: ReleaseCheckOptions) -> Result<()> {
    let repo_root = std::env::current_dir().context("failed to read current directory")?;
    let cargo_version = read_package_version(&repo_root)?;
    let expected = resolve_version(options.version.as_deref(), &cargo_version)?;
    let checks = collect_metadata_checks(&repo_root, &expected, &cargo_version)?;

    print_metadata_checks(&checks);

    let failures = checks
        .iter()
        .filter(|check| !check.ok)
        .map(|check| check.detail.as_str())
        .collect::<Vec<_>>();

    if !failures.is_empty() {
        bail!(
            "Release metadata checks failed:\n- {}",
            failures.join("\n- ")
        );
    }

    println!("Release metadata checks passed for {}", expected.tag);

    if options.metadata_only {
        return Ok(());
    }

    run_release_commands(&repo_root, &expected)
}

pub fn resolve_version(version: Option<&str>, cargo_version: &str) -> Result<ReleaseVersion> {
    let raw = version.unwrap_or(cargo_version).trim();
    let number = raw.strip_prefix('v').unwrap_or(raw);

    if number.is_empty() {
        bail!("release version cannot be empty");
    }

    Ok(ReleaseVersion {
        number: number.to_string(),
        tag: format!("v{number}"),
    })
}

pub fn read_package_version(repo_root: &Path) -> Result<String> {
    let cargo_toml_path = repo_root.join("Cargo.toml");
    let cargo_toml = fs::read_to_string(&cargo_toml_path)
        .with_context(|| format!("failed to read {}", cargo_toml_path.display()))?;
    let cargo_toml: toml::Value = toml::from_str(&cargo_toml)
        .with_context(|| format!("failed to parse {}", cargo_toml_path.display()))?;

    cargo_toml
        .get("package")
        .and_then(|package| package.get("version"))
        .and_then(|version| version.as_str())
        .map(ToOwned::to_owned)
        .context("Cargo.toml is missing package.version")
}

pub fn collect_metadata_checks(
    repo_root: &Path,
    expected: &ReleaseVersion,
    cargo_version: &str,
) -> Result<Vec<ReleaseCheck>> {
    let changelog = read_optional(repo_root.join("CHANGELOG.md"))?;
    let install_doc = read_optional(repo_root.join("docs").join("install.md"))?;
    let install_script = read_optional(repo_root.join("install.sh"))?;
    let install_ps1 = read_optional(repo_root.join("install.ps1"))?;
    let uninstall_script = read_optional(repo_root.join("uninstall.sh"))?;
    let uninstall_ps1 = read_optional(repo_root.join("uninstall.ps1"))?;
    let shell_config_rs = read_optional(
        repo_root
            .join("src")
            .join("commands")
            .join("shell_config.rs"),
    )?;
    let release_notes_path = repo_root
        .join("docs")
        .join("releases")
        .join(format!("{}.md", expected.tag));

    let mut checks = Vec::new();

    if cargo_version == expected.number {
        checks.push(ReleaseCheck::pass(
            "Cargo.toml",
            format!("package version is {}", expected.number),
        ));
    } else {
        checks.push(ReleaseCheck::fail(
            "Cargo.toml",
            format!(
                "Cargo.toml package version is {}, expected {}",
                cargo_version, expected.number
            ),
        ));
    }

    let tag_heading = format!("## {}", expected.tag);
    let has_heading = changelog
        .lines()
        .any(|line| line == tag_heading || line.starts_with(&format!("{} ", tag_heading)));

    if has_heading {
        checks.push(ReleaseCheck::pass(
            "CHANGELOG.md",
            format!("has a {} release heading", expected.tag),
        ));
    } else {
        checks.push(ReleaseCheck::fail(
            "CHANGELOG.md",
            format!("CHANGELOG.md is missing a {} release heading", expected.tag),
        ));
    }

    if release_notes_path.exists() {
        checks.push(ReleaseCheck::pass(
            "release notes",
            format!(
                "{} exists",
                display_relative(repo_root, &release_notes_path)
            ),
        ));
    } else {
        checks.push(ReleaseCheck::fail(
            "release notes",
            format!(
                "{} is missing",
                display_relative(repo_root, &release_notes_path)
            ),
        ));
    }

    if install_doc.contains(&format!("GIT_WARP_VERSION={}", expected.tag)) {
        checks.push(ReleaseCheck::pass(
            "docs/install.md",
            format!("mentions GIT_WARP_VERSION={}", expected.tag),
        ));
    } else {
        checks.push(ReleaseCheck::fail(
            "docs/install.md",
            format!(
                "docs/install.md does not mention GIT_WARP_VERSION={}",
                expected.tag
            ),
        ));
    }

    let install_sh_missing = install_sh_missing_parts(&install_script);
    if install_sh_missing.is_empty() {
        checks.push(ReleaseCheck::pass(
            "install.sh",
            "resolves GIT_WARP_VERSION dynamically, verifies the archive checksum, and honors the skip env",
        ));
    } else {
        checks.push(ReleaseCheck::fail(
            "install.sh",
            format!(
                "install.sh must keep the ${{GIT_WARP_VERSION:-...}} override, call api.github.com/repos/.../releases/latest, run shasum -a 256 -c or sha256sum -c, and honor GIT_WARP_SKIP_CHECKSUM; missing: {}",
                install_sh_missing.join(", ")
            ),
        ));
    }

    let install_ps1_requirements: &[(&str, &str)] = &[
        ("api.github.com/repos/", "the GitHub releases API lookup"),
        (
            "$env:GIT_WARP_VERSION",
            "the $env:GIT_WARP_VERSION override",
        ),
        ("Get-FileHash", "the Get-FileHash checksum step"),
        ("-Algorithm SHA256", "the SHA256 algorithm flag"),
        (
            "GIT_WARP_SKIP_CHECKSUM",
            "the GIT_WARP_SKIP_CHECKSUM / -SkipChecksum opt-out",
        ),
    ];
    checks.push(content_guard(
        "install.ps1",
        &install_ps1,
        install_ps1_requirements,
        "install.ps1 must keep the GitHub releases lookup, $env:GIT_WARP_VERSION override, Get-FileHash -Algorithm SHA256 verification, and GIT_WARP_SKIP_CHECKSUM / -SkipChecksum opt-out",
    ));

    let uninstall_ps1_requirements: &[(&str, &str)] = &[
        (
            "hooks-remove --level user",
            "the user-level hooks-remove call",
        ),
        (
            "GIT_WARP_INSTALL_ROOT",
            "the GIT_WARP_INSTALL_ROOT / -InstallRoot handling",
        ),
    ];
    checks.push(content_guard(
        "uninstall.ps1",
        &uninstall_ps1,
        uninstall_ps1_requirements,
        "uninstall.ps1 must keep the warp hooks-remove --level user invocation and honor -InstallRoot / GIT_WARP_INSTALL_ROOT",
    ));

    let uninstall_sh_requirements: &[(&str, &str)] = &[(
        "hooks-remove --level user",
        "the user-level hooks-remove call",
    )];
    checks.push(content_guard(
        "uninstall.sh",
        &uninstall_script,
        uninstall_sh_requirements,
        "uninstall.sh must keep the warp hooks-remove --level user invocation",
    ));

    let cli_ps_requirements: &[(&str, &str)] = &[
        ("function warp_cd", "the PowerShell warp_cd function"),
        (
            "Register-ArgumentCompleter -CommandName warp",
            "the Register-ArgumentCompleter block",
        ),
    ];
    checks.push(content_guard(
        "src/commands/shell_config.rs",
        &shell_config_rs,
        cli_ps_requirements,
        "src/commands/shell_config.rs must keep the PowerShell shell-config emitter (warp_cd + Register-ArgumentCompleter)",
    ));

    Ok(checks)
}

fn install_sh_missing_parts(script: &str) -> Vec<&'static str> {
    let mut missing = Vec::new();
    if !script.contains("${GIT_WARP_VERSION:-") {
        missing.push("the ${GIT_WARP_VERSION:-...} override");
    }
    if !script.contains("api.github.com/repos/") {
        missing.push("the GitHub releases API lookup");
    }
    if !script.contains("shasum -a 256 -c") && !script.contains("sha256sum -c") {
        missing.push("the shasum -a 256 -c or sha256sum -c checksum verification");
    }
    if !script.contains("GIT_WARP_SKIP_CHECKSUM") {
        missing.push("the GIT_WARP_SKIP_CHECKSUM opt-out guard");
    }
    missing
}

fn content_guard(
    label: &'static str,
    content: &str,
    requirements: &[(&str, &str)],
    fail_summary: &str,
) -> ReleaseCheck {
    let missing: Vec<&str> = requirements
        .iter()
        .filter(|(needle, _)| !content.contains(needle))
        .map(|(_, description)| *description)
        .collect();

    if missing.is_empty() {
        ReleaseCheck::pass(label, format!("{label} keeps its release-critical logic"))
    } else {
        ReleaseCheck::fail(
            label,
            format!("{fail_summary}; missing: {}", missing.join(", ")),
        )
    }
}

fn read_optional(path: PathBuf) -> Result<String> {
    match fs::read_to_string(&path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

fn print_metadata_checks(checks: &[ReleaseCheck]) {
    println!("Release metadata:");

    for check in checks {
        let status = if check.ok { "ok" } else { "missing" };
        println!("- {status}: {} - {}", check.label, check.detail);
    }
}

fn run_release_commands(repo_root: &Path, expected: &ReleaseVersion) -> Result<()> {
    let binary_name = warp_executable_name();
    let release_binary = repo_root.join("target").join("release").join(&binary_name);
    let release_binary = release_binary.to_string_lossy().into_owned();
    let warp_label = format!("./target/release/{binary_name}");

    let steps = [
        ReleaseCommand::new(
            "cargo fmt --all -- --check",
            "cargo",
            &["fmt", "--all", "--", "--check"],
        ),
        ReleaseCommand::new("git diff --check", "git", &["diff", "--check"]),
        ReleaseCommand::new(
            "cargo clippy --all-targets --locked -- -D warnings",
            "cargo",
            &[
                "clippy",
                "--all-targets",
                "--locked",
                "--",
                "-D",
                "warnings",
            ],
        ),
        ReleaseCommand::new("cargo test --locked", "cargo", &["test", "--locked"]),
        ReleaseCommand::new(
            "cargo build --release --bin warp",
            "cargo",
            &["build", "--release", "--bin", "warp"],
        ),
        ReleaseCommand::new(
            format!("{warp_label} --version"),
            release_binary.as_str(),
            &["--version"],
        ),
        ReleaseCommand::new(
            format!("{warp_label} --help"),
            release_binary.as_str(),
            &["--help"],
        ),
        ReleaseCommand::new(
            format!("{warp_label} switch --help"),
            release_binary.as_str(),
            &["switch", "--help"],
        ),
        ReleaseCommand::new(
            format!("{warp_label} cleanup --help"),
            release_binary.as_str(),
            &["cleanup", "--help"],
        ),
        ReleaseCommand::new(
            format!("{warp_label} doctor"),
            release_binary.as_str(),
            &["doctor"],
        ),
    ];

    println!("Release command checks:");

    for step in steps {
        run_step(repo_root, step)?;
    }

    println!("Release checks passed for {}", expected.tag);
    Ok(())
}

struct ReleaseCommand<'a> {
    label: String,
    program: &'a str,
    args: &'a [&'a str],
}

impl<'a> ReleaseCommand<'a> {
    fn new(label: impl Into<String>, program: &'a str, args: &'a [&'a str]) -> Self {
        Self {
            label: label.into(),
            program,
            args,
        }
    }
}

fn run_step(repo_root: &Path, step: ReleaseCommand<'_>) -> Result<()> {
    println!("$ {}", step.label);
    let status = Command::new(step.program)
        .args(step.args)
        .current_dir(repo_root)
        .status()
        .with_context(|| format!("failed to run {}", step.label))?;

    if !status.success() {
        bail!("{} failed with status {}", step.label, status);
    }

    Ok(())
}

fn display_relative(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}
