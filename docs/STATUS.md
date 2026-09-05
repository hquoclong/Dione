# ADE Status (living file — update at the end of every task)

## Current milestone

M2 — Fleet multi-agent (not started).

## Last commit

- `694d7bf` docs: project docs + opencode skills + instructions
- Pushed to `hquoclong/Dione` (`main`): M0 + M1a + M1b + docs live on GitHub.
- M1 verified: `cargo check` pass, clippy 0 warnings, 15/15 tests pass,
  Xvfb smoke shows `ADE — Agentic IDE` 1440x900 with no panic.

## Next up

1. M2 slice 1: real git worktree ops in `worktree.rs` + tests.
2. Adopt `main` as local branch name (currently `master` tracking
   `origin/main`) — optional cleanup.

## Blockers

- Live LLM test (Tier B) needs a provider key; Tier A (no LLM) untested
  until `opencode serve` is exercised with `ADE_EXTERNAL_SERVER_URL`.
