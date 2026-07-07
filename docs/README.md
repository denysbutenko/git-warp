# Git-Warp Documentation

This directory contains user-facing guides, technical notes, and historical
planning material for Git-Warp.

## Start Here

- [Install Git-Warp](install.md): one-command install, PATH setup, and
  version/install-directory options.
- [Release Check](release-check.md): release metadata validation and pre-tag
  smoke checks.
- [User Guide](user-guide.md): daily commands, configuration, and
  troubleshooting.
- [Technical Overview](technical-overview.md): architecture, modules, and
  implementation notes.
- [Changelog](../CHANGELOG.md): release notes and verification commands.
- [Release Notes (v0.4.0)](releases/v0.4.0.md): pasteable notes for the
  `v0.4.0` GitHub release.
- [Release Notes (v0.3.0)](releases/v0.3.0.md): pasteable notes for the
  `v0.3.0` GitHub release.
- [Release Notes (v0.2.0)](releases/v0.2.0.md): pasteable notes for the
  `v0.2.0` GitHub release.
- [Root README](../README.md): short project overview and verified quick start.

## Historical Planning

- [archive/implement-plan-v1.md](archive/implement-plan-v1.md): early
  pre-v0.1.0 implementation plan.
- [archive/implement-plan-v2.md](archive/implement-plan-v2.md): later
  pre-v0.1.0 implementation plan.

These predate v0.1.0 and are kept for historical context only. Specific
milestones, dependencies, and API choices in them no longer reflect the
shipped surface. For current behavior, check `warp --help`,
`warp <command> --help`, [Technical Overview](technical-overview.md), and the
[Changelog](../CHANGELOG.md).

## Common Paths

### First Setup

```bash
curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.sh | sh
warp --version
warp doctor
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
