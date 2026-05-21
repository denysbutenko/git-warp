# Changelog

## Unreleased

### Added

- `warp config --interactive` (`-i`) launches an in-process TUI for editing
  every section of the config: section navigation, boolean toggles, inline
  edits with validation, atomic save, and unsaved-change confirmation on quit.
  List-valued fields render read-only and still require editing the TOML file
  directly.

## v0.3.0 - 2026-05-06

Second public Git-Warp release. Adds per-worktree cleanup eligibility output,
runtime/scope hook diagnostics, ordered `warp ls`, batch removal in the bare-
`warp` switcher, branch-source classification in `warp switch`, an
`uninstall.sh` script, and `warp release-check` for maintainers.

Release notes: [docs/releases/v0.3.0.md](docs/releases/v0.3.0.md)

### Added

- `warp release-check` validates release metadata (`Cargo.toml`, `CHANGELOG.md`,
  `docs/releases/vX.Y.Z.md`, `docs/install.md`, `install.sh`) and runs the
  maintainer release verification flow before tagging.
- Prebuilt release binary workflow for macOS and Linux targets.
- Root `install.sh` installs prebuilt release binaries by default, with Cargo
  available only as an explicit fallback.
- `uninstall.sh` removes the default install, lists other detected `warp`
  binaries without touching them, and supports `--dry-run`.
- `warp doctor` Install check lists detected `warp` binaries with versions and
  warns when more than one is on `PATH`.
- Dedicated install documentation with one-command setup, PATH guidance, custom
  install locations, pinned versions, supported binary targets, and
  upgrade/uninstall flows.
- `warp ls` orders rows by current → primary → dirty → busy → detached → clean,
  adds a one-line summary header, and uses distinct row icons while keeping the
  existing label tokens for script consumers.
- `warp cleanup` (and `--dry-run cleanup`) prints the resolved base branch,
  per-worktree skip reasons (primary, base, protected, detached, no branch),
  mode-excluded entries, candidate tags (`merged` / `identical` / `no remote` /
  `all-mode`), and a `process-busy` flag from a pre-confirm process preview.
- `warp switch` and the bare interactive switcher classify the target as
  existing worktree, local branch, remote branch, or new branch and surface a
  labeled status line; remote-only targets create a local tracking branch from
  the remote ref instead of branching from `HEAD`.
- Bare-`warp` switcher rows show a `local-only` badge for branches with no
  matching ref on any remote (skipped when the repo has no remotes).
- Bare-`warp` switcher supports multi-select (`Space` / `a`) and batch removal,
  with structured reporting of removed, skipped, and failed worktrees.
- `warp hooks-status` and `warp doctor` diagnose each runtime and scope
  independently and report `Complete`, `Partial`, `Missing`, and `Conflicting`
  states with the absolute settings path and the exact
  `warp hooks-install --level <scope> --runtime <runtime>` repair command.
- `warp cleanup --force` can remove dirty worktrees when the user explicitly
  bypasses safety checks.

### Changed

- `warp agents` dashboard history is faster and uses less memory on long
  sessions.
- `install.sh` reports existing `warp` binaries before install, warns when a
  different `warp` shadows the freshly installed one on `PATH`, and prints
  precise next steps on download or extraction failures.

### Fixed

- `warp switch` reuses an existing branch's worktree instead of failing with a
  duplicate-branch error.
- Worktree path root resolution now picks the right repository root when
  Git-Warp is invoked from nested directories.

### Documentation

- README, install docs, user guide, and documentation index updated for the
  shipped command surface.
- Release notes available as a standalone Markdown file at
  `docs/releases/v0.3.0.md` for GitHub release publishing.

### Verification

- `cargo fmt --check`
- `cargo test`
- `cargo build --release --bin warp`
- `./target/release/warp --version`
- `./target/release/warp doctor`

## v0.2.0 - 2026-04-26

First public Git-Warp release.

Release notes: [docs/releases/v0.2.0.md](docs/releases/v0.2.0.md)

### Added

- Worktree switching with `warp switch <branch>` and the short `warp <branch>`
  form.
- Interactive switcher when running bare `warp`.
- Terminal handoff modes for new tabs, new windows, current-shell commands,
  inplace `cd` output, and echo-only scripting.
- Worktree listing with primary, current, dirty, detached, and busy state.
- Cleanup flows with dry-run, interactive selection, protected branches,
  process checks, and force/kill controls.
- `warp doctor` setup checks and recovery guidance.
- Claude/Codex hook installation, hook status checks, and the `warp agents`
  dashboard for local session visibility.
- pnpm post-create setup for pnpm repositories.
- Shell completion generation for Bash, Zsh, and Fish.

### Fixed

- Cleanup candidate analysis now respects protected branches and avoids treating
  base-branch self-merges as removable work.
- Primary worktree detection now handles normal repository roots.
- Process scanning now reports missing worktree paths as errors.
- Path rewriting now handles Unicode text without treating it as binary data.

### Documentation

- README, user guide, and documentation index now match the shipped command
  surface.
- Release notes are available as a standalone Markdown file for GitHub release
  publishing.

### Verification

- `cargo fmt --check`
- `git diff --check`
- `cargo test`
- `cargo build --release --bin warp`
- `./target/release/warp --version`
