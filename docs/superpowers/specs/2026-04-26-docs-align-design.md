# Docs Alignment Design

GitHub issue #10, "[P2] Align README and docs with the shipped feature set",
asks the public docs to stop overstating readiness, remove missing or broken
document references, and keep quick-start examples on implemented commands.

## Context

The current CLI exposes `switch`, `ls`, `cleanup`, `config`, `agents`,
`doctor`, hook commands, and `shell-config`. `config --edit` is implemented as
"open the config file in an editor", while the old docs implied broader
interactive configuration and linked to pages that do not exist. The README and
docs index also used production-ready and success-story language that reads more
like a launch page than setup documentation.

## Approach

Use a docs-only patch. Replace the root README, docs index, and user guide with
shorter current-state documentation grounded in the real CLI help. Keep existing
historical documents, but label them as historical context so users do not treat
old plans as the shipped command surface.

## Decisions

- Keep examples to verified commands: `warp doctor`, `warp switch`, short-form
  branch switching, `warp ls`, `warp cleanup`, `warp config`, hooks, `agents`,
  and `shell-config`.
- Describe the agent dashboard as optional visibility based on hooks and local
  session history, not guaranteed live monitoring in every setup.
- Describe Copy-on-Write as macOS/APFS acceleration with traditional Git
  fallback, not universal instant readiness.
- Remove links to missing standalone pages such as quick reference,
  troubleshooting guide, API docs, changelog, and migration guide.
- Keep `config --edit` documented accurately as editor-based config opening.

## Testing

Verification should include:

- CLI help checks for the commands used in examples.
- A local Markdown link check for relative files and anchors in `README.md`,
  `docs/README.md`, and `docs/user-guide.md`.
- `git diff --check`.
