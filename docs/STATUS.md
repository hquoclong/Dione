# ADE Status (living file — update at the end of every task)

## Current milestone

M2 — Fleet multi-agent (not started).

## Last commit

- `66ec918` feat(m1b): ade-ui single-agent — sidebar, timeline, composer,
  context/inspector/diff, permission gate
- M1 verified: `cargo check` pass, clippy 0 warnings, 15/15 tests pass,
  Xvfb smoke shows `ADE — Agentic IDE` 1440x900 with no panic.

## Next up

1. Commit project docs (`docs/`) + opencode skills (this task).
2. Push to GitHub (blocked: no remote yet, `gh` token expired — owner
   must create repo + `gh auth login`).
3. M2 slice 1: real git worktree ops in `worktree.rs` + tests.

## Blockers

- GitHub remote missing (see above).
- Live LLM test (Tier B) needs a provider key; Tier A (no LLM) untested
  until `opencode serve` is exercised with `ADE_EXTERNAL_SERVER_URL`.
