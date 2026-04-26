use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct ReleaseCheckOptions {
    pub version: Option<String>,
    pub metadata_only: bool,
}

struct ReleaseVersion {
    number: String,
    tag: String,
}

struct ReleaseCheck {
    label: &'static str,
    ok: bool,
    detail: String,
}

impl ReleaseCheck {
    fn pass(label: &'static str, detail: impl Into<String>) -> Self {
        Self {
            label,
            ok: true,
            detail: detail.into(),
        }
    }

    fn fail(label: &'static str, detail: impl Into<String>) -> Self {
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

fn resolve_version(version: Option<&str>, cargo_version: &str) -> Result<ReleaseVersion> {
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

fn read_package_version(repo_root: &Path) -> Result<String> {
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

fn collect_metadata_checks(
    repo_root: &Path,
    expected: &ReleaseVersion,
    cargo_version: &str,
) -> Result<Vec<ReleaseCheck>> {
    let changelog = read_optional(repo_root.join("CHANGELOG.md"))?;
    let install_doc = read_optional(repo_root.join("docs").join("install.md"))?;
    let install_script = read_optional(repo_root.join("install.sh"))?;
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

    if changelog.contains(&format!("## {} ", expected.tag))
        || changelog.contains(&format!("## {}\n", expected.tag))
    {
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

    if install_script.contains(&format!(
        "version=\"${{GIT_WARP_VERSION:-{}}}\"",
        expected.tag
    )) {
        checks.push(ReleaseCheck::pass(
            "install.sh",
            format!("defaults to {}", expected.tag),
        ));
    } else {
        checks.push(ReleaseCheck::fail(
            "install.sh",
            format!(
                "install.sh default GIT_WARP_VERSION is not {}",
                expected.tag
            ),
        ));
    }

    Ok(checks)
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
    let release_binary = repo_root.join("target").join("release").join("warp");
    let release_binary = release_binary.to_string_lossy().into_owned();

    let steps = [
        ReleaseCommand::new("cargo fmt --check", "cargo", &["fmt", "--check"]),
        ReleaseCommand::new("cargo test", "cargo", &["test"]),
        ReleaseCommand::new(
            "cargo build --release --bin warp",
            "cargo",
            &["build", "--release", "--bin", "warp"],
        ),
        ReleaseCommand::new(
            "./target/release/warp --version",
            release_binary.as_str(),
            &["--version"],
        ),
        ReleaseCommand::new(
            "./target/release/warp --help",
            release_binary.as_str(),
            &["--help"],
        ),
        ReleaseCommand::new(
            "./target/release/warp switch --help",
            release_binary.as_str(),
            &["switch", "--help"],
        ),
        ReleaseCommand::new(
            "./target/release/warp cleanup --help",
            release_binary.as_str(),
            &["cleanup", "--help"],
        ),
        ReleaseCommand::new(
            "./target/release/warp doctor",
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
    label: &'static str,
    program: &'a str,
    args: &'a [&'a str],
}

impl<'a> ReleaseCommand<'a> {
    fn new(label: &'static str, program: &'a str, args: &'a [&'a str]) -> Self {
        Self {
            label,
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
