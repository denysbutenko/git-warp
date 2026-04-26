# Git-Warp Documentation

This directory contains user-facing guides, technical notes, historical planning
material, and original reference documents for Git-Warp.

## Start Here

- [User Guide](user-guide.md): installation, daily commands, configuration, and
  troubleshooting.
- [Technical Overview](technical-overview.md): architecture, modules, and
  implementation notes.
- [Changelog](../CHANGELOG.md): release notes and verification commands.
- [Release Notes](releases/v0.2.0.md): pasteable notes for the `v0.2.0`
  GitHub release.
- [Root README](../README.md): short project overview and verified quick start.

## Reference Documents

- [autowt.txt](autowt.txt): UX-focused Python predecessor reference.
- [coworktree.txt](coworktree.txt): CoW-focused Go predecessor reference.
- [implement-plan-v1.md](implement-plan-v1.md): early implementation plan.
- [implement-plan-v2.md](implement-plan-v2.md): later implementation plan.

The implementation plans and predecessor references are historical context. The
current CLI behavior is best checked with `warp --help`, `warp <command> --help`,
and the current README/user guide.

## Common Paths

### First Setup

```bash
cargo install --locked --force --git https://github.com/denysbutenko/git-warp --tag v0.2.0 --bin warp git-warp
warp doctor
warp --help
```

### Daily Worktree Flow

```bash
warp switch feature/my-change
warp ls
warp --dry-run cleanup --mode merged
warp cleanup --interactive
```

### Agent Session Visibility

```bash
warp hooks-install --level user --runtime all
warp hooks-status
warp agents
```

Live agent rows require hooks or local session history that Git-Warp can read.
Without those inputs, the dashboard opens with an empty state.

## Find What You Need

- Basics: [User Guide: Quick Start](user-guide.md#quick-start)
- Configuration: [User Guide: Configuration](user-guide.md#configuration)
- Troubleshooting: [User Guide: Troubleshooting](user-guide.md#troubleshooting)
- Copy-on-Write internals:
  [Technical Overview: Copy-on-Write Implementation](technical-overview.md#copy-on-write-implementation)
- Process safety:
  [Technical Overview: Process Management](technical-overview.md#process-management)
- Performance notes:
  [Technical Overview: Performance Benchmarks](technical-overview.md#performance-benchmarks)

## Documentation Maintenance

When editing docs:

- Prefer examples verified against the current CLI help.
- Do not link to missing pages or planned guides.
- Mark historical material as historical so users do not treat old plans as the
  shipped command surface.
- Keep setup examples safe: use `warp doctor`, `--dry-run`, and non-destructive
  commands before cleanup examples.
