# Unified Agent Dashboard Design

Date: 2026-04-23
Repo: `/Users/deryzh/dev/git-warp`
Command surface: `warp agents`

## Summary

Upgrade `warp agents` from a demo-only TUI into a real unified dashboard that shows:

- live Claude and Codex agent activity from Git-Warp hook status files
- recent Claude and Codex sessions discovered from their local session stores
- sessions scoped to the current repository and its worktrees

The default view includes both open/live sessions and recently closed sessions from the last 7 days.

## Goals

- Show Claude and Codex sessions together in one dashboard.
- Prefer live status from existing Git-Warp hook files when available.
- Include recent closed sessions so the dashboard remains useful even when an agent is no longer running.
- Restrict results to the current repository and its worktree locations.
- Replace the current hard-coded mock data in `warp agents`.

## Non-Goals

- Adding remote or team-wide monitoring.
- Persisting a new Git-Warp-owned session database.
- Adding new CLI flags in the first pass.
- Resuming, attaching to, or controlling sessions from the dashboard.

## Product Decisions

- Dashboard scope: unified Claude + Codex view.
- History window: 7 days by default.
- Default ordering: live sessions first, then recent closed sessions by newest activity.
- Initial command surface: keep using `warp agents` with no required new arguments.

## Current Reality

Today, the repo already has part of the runtime foundation:

- `warp hooks-install --runtime claude|codex|all` can install hooks for both runtimes.
- Hooks write live status JSON into:
  - `.claude/git-warp/status`
  - `.codex/git-warp/status`
- `warp agents` still renders hard-coded demo rows in the TUI and does not read real session data.

Real local data sources are also available on disk:

- Codex sessions under `~/.codex/sessions/**/*.jsonl`
- Claude sessions under `~/.claude/projects/**/*.jsonl`

## Data Sources

### 1. Live status files

For each repo root and worktree path, check for:

- `<path>/.claude/git-warp/status`
- `<path>/.codex/git-warp/status`

These files provide the most accurate current state when present, because they are written by runtime hooks during active work.

Expected status values currently include:

- `working`
- `processing`
- `waiting`
- `subagent_complete`

### 2. Codex session store

Read `~/.codex/sessions/**/*.jsonl` and extract session metadata from `session_meta` rows. The fields we can rely on are:

- `payload.id`
- `payload.cwd`
- `payload.timestamp`
- `payload.originator`
- `payload.agent_nickname` when present
- `payload.agent_role` when present

For recent-activity timestamps, use the newest timestamp seen in the file when needed.

### 3. Claude session store

Read `~/.claude/projects/**/*.jsonl` and extract session information from top-level events. Useful fields include:

- `sessionId`
- `cwd`
- `timestamp`
- `gitBranch` when present

Claude logs do not appear to provide the same clean single `session_meta` record as Codex, so parsing must tolerate mixed event shapes.

## Repository Scoping

The dashboard must only show sessions belonging to the current repo and its worktrees.

Repo affinity rule:

1. Resolve the current repo root with the existing git repo discovery code.
2. Build the allowed path set from:
   - the repo root itself
   - all known worktree paths returned by Git-Warp's git worktree listing
3. Keep a session only when its `cwd` is equal to or nested under one of those allowed paths.

This keeps the dashboard focused on the active repo and avoids cross-repo noise from the user's global session stores.

## Normalized Model

Add a shared runtime-agnostic model for the TUI, for example:

```rust
pub struct AgentSessionSummary {
    pub runtime: AgentRuntime,
    pub session_id: Option<String>,
    pub cwd: PathBuf,
    pub branch: Option<String>,
    pub agent_label: String,
    pub state: AgentSessionState,
    pub last_activity: DateTime<Local>,
    pub is_live: bool,
    pub source: AgentSessionSource,
}
```

Support enums:

- `AgentRuntime`: `Claude | Codex`
- `AgentSessionState`: `Working | Processing | Waiting | Completed | Recent | Unknown`
- `AgentSessionSource`: `LiveStatus | SessionStore | Merged`

Notes:

- `session_id` is optional because some Claude-derived history rows may not expose one cleanly.
- `agent_label` should show Codex nickname/role when available, otherwise fall back to `Codex`.
- Claude rows can default to `Claude` unless a better label is available later.

## Discovery Pipeline

Build the unified dashboard data in two passes.

### Pass 1: collect live status

- Inspect repo root and each worktree path for runtime status files.
- Parse status JSON.
- Produce provisional `AgentSessionSummary` rows marked `is_live = true`.
- Use the file modification time as a fallback `last_activity` when the JSON timestamp is missing or invalid.

### Pass 2: collect recent sessions

- Scan the Codex and Claude session stores.
- Parse only sessions from the last 7 days.
- Filter to sessions whose `cwd` belongs to this repo/worktree set.
- Produce provisional rows marked `is_live = false`.

### Merge and dedupe

Merge rows by a stable key:

- prefer `runtime + session_id` when a session id is present
- otherwise fall back to `runtime + cwd`

Precedence rules:

1. Live status file wins for state.
2. Session-store row fills metadata such as agent nickname, role, branch, or better timestamps.
3. If both exist, emit one merged row, not duplicates.

## State Mapping

Map raw runtime values into shared UI states:

- `working` -> `Working`
- `processing` -> `Processing`
- `waiting` -> `Waiting`
- `subagent_complete` -> `Completed`
- recent session with no live status -> `Recent`
- anything unrecognized -> `Unknown`

This preserves current runtime semantics without forcing the TUI to understand each runtime's raw strings.

## TUI Behavior

Keep the command as `warp agents`, but replace the current fake activity list with real session summaries.

### List view

Each row should show:

- state indicator
- runtime
- branch or worktree name
- agent label
- relative last-activity time

Sorting:

- live rows first
- then by `last_activity` descending

### Detail pane

Selecting a row should show:

- full cwd
- session id
- runtime
- branch if known
- live vs recent-only
- exact timestamp
- source data origin (`LiveStatus`, `SessionStore`, `Merged`)

### Empty state

If no sessions are found, render a useful empty state instead of a blank table:

- no live or recent Claude/Codex sessions found for this repo in the last 7 days
- suggest `warp hooks-install --runtime all --level user` if live monitoring is missing

## Error Handling

The dashboard must degrade gracefully.

- Missing Claude or Codex home directories should not fail the command.
- Malformed JSON lines should be skipped with debug logging.
- A broken file in one runtime must not hide valid rows from the other runtime.
- Missing status files should simply mean there is no live state for that path.

## Module Boundaries

Do not put discovery/parsing logic directly into the TUI rendering file.

Preferred split:

- new agent discovery module for reading status files and session stores
- small normalized session model shared with the TUI
- `src/tui.rs` limited to rendering and keyboard interaction

This keeps the TUI testable and prevents parsing concerns from leaking into view code.

## Testing Strategy

Add unit coverage for:

- Codex session parsing from representative `session_meta` lines
- Claude session parsing from representative project log lines
- repo/worktree path filtering
- 7-day cutoff filtering
- merge precedence between live status rows and session-store rows
- ordering: live first, then newest recent sessions
- state mapping from runtime-specific values into shared UI states

Keep tests local and deterministic by using temporary directories and fixture content instead of real home-directory state.

## Risks

- Claude log formats are less uniform than Codex session metadata, so the parser must be conservative.
- Large session stores can get expensive to scan; the first implementation should stay correct first, then optimize if needed.
- A single status file per worktree/runtime may represent the current active state but not multiple concurrent agents in the same path. That limitation is acceptable for the first pass.

## Future Extensions

If the first pass lands well, later work could add:

- optional `--days` or `--runtime` filters
- action keys for resume/attach flows
- file watching via `notify` for live refresh instead of manual refresh loops
- a Warp-owned cache/index only if scan cost becomes a real problem

## Acceptance Criteria

- `warp agents` shows real Claude and Codex rows instead of mock data.
- Live hook status is reflected when `.claude/git-warp/status` or `.codex/git-warp/status` exists in the repo or its worktrees.
- Recent closed Claude and Codex sessions from the last 7 days appear when they belong to the current repo/worktrees.
- Duplicate live/history rows collapse into one merged row.
- The command still works when one runtime is not installed or has no local history.
