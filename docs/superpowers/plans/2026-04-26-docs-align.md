# Docs Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Align Git-Warp README and docs with the current shipped CLI surface.

**Architecture:** This is a docs-only change. The root README becomes the short
source of truth, `docs/README.md` becomes a real index of existing documents,
and `docs/user-guide.md` becomes a concise command guide grounded in current
help output.

**Tech Stack:** Markdown, Git-Warp CLI help, shell verification.

---

### Task 1: Verify Current CLI Surface

**Files:**
- Read: `src/cli.rs`
- Read: `README.md`
- Read: `docs/README.md`
- Read: `docs/user-guide.md`

- [x] **Step 1: Fetch command help**

Run:

```bash
cargo run --quiet -- --help
cargo run --quiet -- config --help
cargo run --quiet -- cleanup --help
cargo run --quiet -- hooks-install --help
```

Expected: command help lists the shipped command names and options used in the
docs examples.

- [x] **Step 2: Identify stale documentation**

Run:

```bash
rg -n "production|success|AI-powered|Quick Reference|API Documentation|Migration Guide|Change Log|interactive config|Live Dashboard|LICENSE" README.md docs/*.md
```

Expected: stale launch-style claims, missing page references, and broken links
are visible before editing.

### Task 2: Rewrite Public Docs

**Files:**
- Modify: `README.md`
- Modify: `docs/README.md`
- Modify: `docs/user-guide.md`

- [x] **Step 1: Rewrite README**

Replace the launch-style README with a concise current-state overview covering
install, quick start, terminal modes, cleanup, agent dashboard, config, shell
integration, development checks, docs links, status, and license.

- [x] **Step 2: Rewrite docs index**

Keep only links to existing docs and valid anchors. Mark implementation plans
and predecessor documents as historical context.

- [x] **Step 3: Rewrite user guide**

Keep examples to shipped commands and explain limits clearly: CoW is
macOS/APFS-specific acceleration, agent visibility depends on hooks/session
history, and `config --edit` opens the config in an editor.

### Task 3: Verify Documentation

**Files:**
- Read: `README.md`
- Read: `docs/README.md`
- Read: `docs/user-guide.md`

- [x] **Step 1: Check local links**

Run a Markdown link checker over the edited files. Expected: no missing local
files and no missing local anchors.

- [x] **Step 2: Check formatting whitespace**

Run:

```bash
git diff --check
```

Expected: exit 0.

- [x] **Step 3: Re-run command help used as source of truth**

Run:

```bash
cargo run --quiet -- --help
cargo run --quiet -- config --help
cargo run --quiet -- cleanup --help
cargo run --quiet -- hooks-install --help
```

Expected: exit 0 for each command; warnings are allowed if they are existing
Rust warnings unrelated to the docs change.
