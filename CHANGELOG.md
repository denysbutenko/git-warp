# Changelog

## Unreleased

### Added

- Ship MIT `LICENSE` at repo root, matching the `license = "MIT"` field
  already declared in `Cargo.toml`. (#246)
- `warp __complete branches` now includes remote-tracking branches (exposed by
  their trailing name so `origin/feature-x` completes as `feature-x`) and tags,
  deduped against local branches. (#254)

### Changed

- Split the monolithic `src/cli.rs` (~2900 lines) into per-subcommand
  modules under `src/commands/{switch,ls,cleanup,config,doctor,agents,
  hooks,shell_config,complete,util}.rs`. `src/cli.rs` now only defines
  the Clap `Cli` / `Commands` types and dispatches into the modules.
  Release-check now guards the PowerShell shell-config emitter in
  `src/commands/shell_config.rs`. (#142)

### Fixed

- `abbreviate_path` no longer collapses a sibling of the home directory
  (e.g. `/Users/alice-scratch` → `~-scratch`) in the worktree switcher's
  removal messages. The strip only applies when the remainder is empty or
  starts with a path separator. (#236)
- `warp release-check` guards the `-SkipChecksum` / `GIT_WARP_SKIP_CHECKSUM`
  opt-out in `install.ps1`, mirroring the existing `install.sh` guard so a
  release cannot ship a Windows installer that dropped the opt-out. (#231)
- `warp cleanup` and the switcher's multi-select batch removal now exit
  non-zero when any worktree removal (or process-termination step gating a
  removal) fails, so `set -e`, `&&` chains, and CI jobs no longer treat a
  partial cleanup as success. The human-readable summary line still prints
  before the error propagates. (#275)

### Dependencies

- Bump `windows-sys` from 0.60.2 to 0.61.2. (#243)
- Bump the cargo minor-and-patch group across 3 updates
  (`clap`, `ignore`, `sysinfo`). (#245)
- Bump the cargo minor-and-patch group across 6 updates
  (`clap`, `ignore`, `serde`, `serde_json`, `anyhow`, `thiserror`). (#259)
- Bump `lru` from 0.18.0 to 0.18.2 to pick up `RUSTSEC-2026-0253` (panic-safety
  fix in `LruCache::pop()`). (#260)

## v0.5.0 - 2026-07-08

Follow-up release to v0.4.0. It finishes the Windows shell story with a
PowerShell shell-config emitter, and fixes `warp switch` so a new worktree
overlays only the primary's untracked and ignored files instead of copying the
whole tree.

Release notes: [docs/releases/v0.5.0.md](docs/releases/v0.5.0.md)

### Added

- PowerShell shell-config: `warp shell-config powershell` (alias `pwsh`) emits a
  `warp_cd` helper and a `Register-ArgumentCompleter` block for branch
  completion. `warp doctor` recognizes PowerShell via `PSModulePath` and points
  at the `warp shell-config powershell` snippet and `$env:PATH` instead of the
  old "not yet shipped" placeholder. (#219)
- `[cow] exclude` config knob: a list of untracked/ignored paths to skip when a
  new worktree overlays the primary. It defaults to build and cache output;
  `node_modules` is still copied so a fresh worktree stays runnable. (#227)

### Changed

- Worktree removal prints a single terser line.

### Fixed

- `warp switch` keeps the real `git worktree add` and CoW-overlays only the
  primary worktree's untracked and ignored files (`git status -z --ignored`).
  The previous path did `git worktree add`, `rm -rf`, then cloned the entire
  primary tree — `.git` and build output included — which copied the whole
  object store for nothing, walked all of it during path rewriting, and left
  the worktree with a copied `.git` instead of a linked-worktree pointer.
  Rewriting is now scoped to the overlaid files so tracked files never gain
  spurious diffs, and a failed overlay is removed so no half-copied state
  survives. (#227)
- `warp cleanup --kill` on Windows honors `[process].kill_timeout`. The Windows
  branch of `ProcessManager::terminate_single_process` fires a single
  `taskkill /T` for the graceful attempt, waits on the process handle via
  `WaitForSingleObject(kill_timeout)`, then falls back to `TerminateProcess`
  through the same handle instead of a second `taskkill /F` spawn. (#212)
- `uninstall.ps1` honors `-InstallRoot` / `GIT_WARP_INSTALL_ROOT`, mirroring
  `install.ps1`, so a Cargo install with a custom root uninstalls from the right
  place. (#218)
- `warp release-check` guards `install.sh` and `uninstall.sh` contents, so a
  release can't ship scripts that dropped the checksum verification or the
  `hooks-remove` cleanup. (#214)

### Dependencies

- Bump `crossbeam-epoch` to 0.9.20 (RUSTSEC-2026-0204).
- Bump the cargo minor-and-patch group across 7 updates. (#224)

## v0.4.0 - 2026-07-02

Third public Git-Warp release. Highlights: Windows support (prebuilt binary,
`install.ps1`, `uninstall.ps1`, Windows-aware `doctor`), Linux Copy-on-Write via
the FICLONE ioctl, a unified bare-`warp` TUI that toggles between the worktree
switcher and the agents dashboard with `Tab`, an interactive config editor and
`warp ls` selector, commit-ish (tag/SHA) switching, and a broad hardening pass
across install, terminal, process, and cross-platform behavior.

Release notes: [docs/releases/v0.4.0.md](docs/releases/v0.4.0.md)

### Added

- Bare `warp` and the agents dashboard are unified in one TUI: press `Tab` in the
  worktree switcher to view live Claude/Codex agent sessions and `Tab` again to
  return, in the same terminal session. The switcher stays the default view;
  `warp agents` still opens the dashboard directly. When `agent.enabled=false`,
  `Tab` shows a notice instead of switching.
- `warp ls --interactive` (`-i`) opens a selection TUI over the worktree list.
- `warp switch` accepts tags and SHAs; commit-ish targets create a local
  branch at the resolved commit so the worktree pins exactly that revision.
  (#88)
- `warp config --interactive` (`-i`) launches an in-process TUI for editing
  every section of the config: section navigation, boolean toggles, inline
  edits with validation, atomic save, and unsaved-change confirmation on quit.
  List-valued fields render read-only and still require editing the TOML file
  directly. (#87)
- Linux Copy-on-Write cloning via the `FICLONE` ioctl on filesystems that
  implement reflinks (btrfs, xfs with `reflink=1`, bcachefs, OCFS2), with a
  functional reflink-support probe that leaves no scratch files behind.
  (#117, #159, #160, #167)
- Windows support: a prebuilt Windows binary, `install.ps1` and `uninstall.ps1`,
  and a Windows-aware `warp doctor` (probes, install dir, shell detection, and
  reinstall hint). (#165, #177, #178)
- Post-create auto-install detects npm, yarn, bun, and Cargo lockfiles in
  addition to pnpm. Detection order is `pnpm-lock.yaml`, `yarn.lock`,
  `bun.lock` (Bun 1.2+), `bun.lockb`, `package-lock.json`, `Cargo.toml`.
  A new `[post_create] auto_install` config knob (also
  `GIT_WARP_POST_CREATE__AUTO_INSTALL`) opts out. (#57, #77, #119)
- Post-create `init_commands` support `{{branch}}`, `{{repo}}`, and `{{path}}`
  placeholders.
- `warp doctor` reports whether the `git` binary is on `PATH` and prints
  the version line on success or a reinstall hint on failure. (#84)
- Agents dashboard gains a `Starting` session state, and hooks register the
  `SessionStart` event.
- `[process]`, `[git].auto_fetch`/`auto_prune`, and `[agent].enabled`/
  `[agent].refresh_rate` config keys now drive runtime behavior:
  `process.kill_timeout` controls SIGTERM grace, `process.auto_kill` resolves
  the cleanup kill flag, `process.check_processes` toggles the busy preview,
  `git.auto_fetch`/`git.auto_prune` gate the cleanup fetch and prune steps,
  `agent.enabled` short-circuits `warp agents`, and `agent.refresh_rate`
  replaces the hardcoded 2 s dashboard interval. (#82)
- Nested `GIT_WARP_*__*` environment variables can override any nested
  config section (e.g. `GIT_WARP_POST_CREATE__AUTO_INSTALL=false`). (#92)
- `uninstall.sh` also removes installed agent hooks and shell-config entries.

### Changed

- The bare-`warp` TUI is refactored into terminal-free view controllers
  (`WorktreeSwitchView`, `AgentsView`) behind a shared shell, and `src/tui.rs`
  is split into per-view modules. Existing behavior is preserved; the shell adds
  the `Tab` toggle.
- `warp cleanup` parallelizes per-worktree probes via rayon and loads branch
  remotes in one bulk `git config --get-regexp` pass instead of per-branch
  shellouts. Candidate order is preserved. (#86)
- Default `protected_branches` is now `["main", "develop"]`; `master` is
  no longer in the offline fallback list. Repos still using `master`
  continue to resolve via `origin/HEAD`; opt back in by adding `master`
  to `config.toml`. (#74)
- `Cargo.toml` is consolidated into a single source of truth for package
  metadata. (#97)
- TUI code updated for `ratatui` 0.30.
- Dropped the unused `agent.claude_hooks` config knob and a Linux overlayfs
  CoW placeholder.

### Fixed

- CoW cloning no longer recurses without bound when the destination lives
  inside the source (for example `<repo>/.worktrees/<branch>`): the worktree
  storage directory on the path to the destination is excluded from the clone,
  so a worktree stored inside the repo no longer copies every sibling worktree
  into itself and fills the disk. (#215)
- Agent hook entries now invoke a hidden `warp __hook-status --runtime <r>
  --status <s>` subcommand instead of an inline POSIX `sh` chain with
  `date -Iseconds`. The single executable invocation parses identically under
  `cmd.exe`, `pwsh`, `bash`, and `dash`, so Windows hooks finally write
  `.claude/git-warp/status` / `.codex/git-warp/status`, and macOS BSD `date`
  no longer truncates `last_activity` to an empty string. Re-run
  `warp hooks-install` to rewrite already-installed entries. (#189)
- `warp --terminal current`, and the `echo`/`inplace` terminal modes, work
  cross-platform on Windows; invalid `--terminal` / `terminal.app` values are
  rejected. (#149, #172, #173)
- TUI strings and docs no longer render Windows-1252 mojibake. (#171)
- `install.sh` verifies the release archive's `.sha256` companion via
  `shasum`/`sha256sum` before extracting; mismatches abort with both digests
  printed. `GIT_WARP_SKIP_CHECKSUM=1` opts out. (#78)
- `install.sh` resolves the latest release tag from the GitHub API when
  `GIT_WARP_VERSION` is unset instead of pinning a stale literal default.
  (#91)
- `install.ps1` detects an existing `warp.exe` and warns on PATH shadowing, and
  honors `-InstallDir` / `-InstallRoot` on its cargo path. (#185, #191, #205)
- `warp release-check` guards `install.ps1` / `uninstall.ps1` contents, honors
  the `.exe` suffix in the binary smoke steps, and mirrors the CI
  fmt/clippy/test gates exactly. (#79, #201, #208)
- `uninstall.sh` detects a `.git` file inside linked git worktrees. (#207)
- Invalid or unknown CLI inputs are rejected at parse time: conflicting
  `--kill`/`--no-kill`, unknown `cleanup --mode`, and unknown
  `hooks-install`/`hooks-remove --level`. (#67, #154, #179)
- `warp --debug` actually enables debug logging. (#147)
- Atomic writes now `fsync` the parent directory on Unix after rename so the
  rename itself survives a crash. Applied to hooks settings, config save,
  path rewriting, and post-create marker writes. (#68, #102)
- Process termination uses `nix::sys::signal::kill` for SIGTERM, liveness
  probes, and SIGKILL instead of shelling out to `kill(1)`. Removes three
  subprocess spawns per terminated PID and surfaces `ESRCH`/`EPERM` distinctly.
  (#83)
- `warp cleanup` polls for graceful exit on a 50 ms cadence with a 10 s
  SIGKILL budget instead of a fixed 2 s sleep, so processes that flush state
  for longer than 2 s are no longer force-killed. (#70)
- Agent heartbeat files (`.claude/git-warp/status`, `.codex/git-warp/status`)
  are added to `.git/info/exclude` on every switch so they stop producing
  per-branch diff churn and merge conflicts. (#94)
- `warp shell-config` derives the completion subcommand list from the clap
  command tree, so the bash/zsh/fish snippets no longer drift; `release-check`
  is now listed. (#66)
- Cleanup selector TUI restores raw mode and the alt screen via the shared
  `TuiTerminalGuard` so `?` early-return and panic unwinds no longer leave
  the terminal scrambled. (#103)
- `PathRewriter` only replaces matches at a path boundary and skips files
  larger than 2 MiB, fixing prefix-collision corruption (e.g. rewriting
  `/foo/repo` inside `/foo/repo-archive`) and bounding memory use; it also
  walks gitignored files while skipping `.git`. (#58, #161)

### Performance

- Process and cleanup scans refresh the process list once per worktree pass
  instead of per query.

### Dependencies

- Bump `thiserror` 1.0.61 → 2.0.18, `toml` 0.8.23 → 0.9.6, `rand` 0.8.5 →
  0.10.1, and `criterion` 0.5.1 → 0.8.2. (#195, #196, #197)
- Bump the cargo minor-and-patch group (11 updates) and refresh stale direct
  dependencies. (#99)
- Drop `gix` in favor of the `git` CLI for repo discovery, and drop the unused
  `notify` dependency.

### CI / Build

- Add an fmt/clippy/test workflow for PRs and `main`, with `windows-latest` in
  the clippy/test matrix; clear the clippy backlog and enforce `-D warnings`.
  (#90)
- Add a `cargo audit` workflow for RUSTSEC advisories and a Dependabot config
  for cargo and github-actions. (#183, #184)
- Run `git diff --check` on PRs and in `release-check`; bump `actions/checkout`
  to v7. (#166)

### Documentation

- Add v0.4.0 release notes and document Linux CoW via the FICLONE ioctl rather
  than `cp --reflink`. (#206)
- Show a `Tab` toggle hint in the switcher and agents help footers.
- Archive pre-v0.1.0 implementation plans under `docs/archive`, drop vendored
  reference dumps, and gitignore `CLAUDE.md`.

### Internal

- Drop unused `GitWarpError` variants. (#155)
- Add unit tests for release-check logic and partial-metadata checks, agents
  `Starting`-state mapping, and process query/refresh behavior; align the
  performance benchmarks with the current API. (#104)
- Broad cross-platform CI hardening for Windows: config isolation in
  integration tests, fake editor and PATH shims, `.cmd` package-manager
  resolution, and manifest/post-create test portability.

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
