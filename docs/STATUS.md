# ADE Status (living file — update at the end of every task)

## Current milestone

M2 — Fleet multi-agent (slices 1+2 done, slice 3: fan-out/compare/merge).

## Last commit

- `90c8a09` feat(m2b): session-worktree link — per-directory clients+pumps,
  fleet sidebar (`495e044` m2a git ops before it)
- Verified: check pass, clippy 0 warnings, 22/22 tests pass (incl. 4 real
  git worktree tests + 3 fleet mapping tests), Xvfb smoke clean.

## Next up

1. M2 slice 3: fan-out 1 prompt → N worktrees, side-by-side diff compare,
   annotate → send back, merge winner.
2. Live Tier A check: run app against real `opencode serve`, create 2
   worktrees, verify per-directory sessions + pumps (needs provider key
   for Tier B prompts).
3. Optional: rename local `master` → `main`.

## Blockers

- Live LLM test (Tier B) needs a provider key; Tier A (no LLM) untested
  until `opencode serve` is exercised with `ADE_EXTERNAL_SERVER_URL`.
