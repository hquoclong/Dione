# ADE Status (living file — update at the end of every task)

## Current milestone

M2 — Fleet multi-agent DONE. Next: live Tier A check, then M3 scoping.

## Last commit

- `7d420e7` feat(m2e): merge winner --no-ff (`04c5dbb` m2d notes,
  `dc209fb` m2c fan-out/compare before it)
- Verified: check pass, clippy 0 warnings, 26/26 tests pass (incl. 6 real
  git tests: create/remove/prune/merge/dirty-guard), Xvfb smoke clean.

## Next up

1. Live Tier A check: run app against real `opencode serve`, create 2
   worktrees via `+ wt`, fan-out a prompt, annotate a diff line, merge —
   needs provider key for prompts (Tier B).
2. M3 scoping: SSH remote worktrees vs packaging vs usage tracking.
3. Optional: rename local `master` → `main`.

## Blockers

- None for build; live LLM verification needs a provider key.
