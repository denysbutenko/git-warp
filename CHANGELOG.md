# Changelog

## Unreleased

### Added

- `warp release-check` validates release metadata and runs the maintainer
  release verification flow before tagging.
- Prebuilt release binary workflow for macOS and Linux targets.
- Root `install.sh` now installs prebuilt release binaries by default, with
  Cargo available only as an explicit fallback.
- Dedicated install documentation with one-command setup, PATH guidance, custom
  install locations, pinned versions, and supported binary targets.

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
