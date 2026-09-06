# ADE Status (living file — update at the end of every task)

## Current milestone

M2 — Fleet multi-agent DONE. Docs v2 (M3–M15 plan) DONE, chưa commit.
Next: M3a `transcript.rs` (UnifiedMessage/Cost + dual-write mirror).

## Last commit (đã verify)

- Docs v2: `00-START-HERE`, `GLOSSARY-VI`, `ARCHITECTURE-v2`,
  `WORKSPACE-VM`, `AGENT-ANY`, `WORKFLOW`, `LABS`, `adr/0001-0004`,
  `ROADMAP.md` mở rộng M3→M15
- Verified: check pass, clippy 0 warnings (ngoài future-incompat của
  dep `proc-macro-error2`), 26/26 tests pass, `cargo fmt --check` sạch.
- Trước đó: `7d420e7` feat(m2e) merge winner --no-ff (`04c5dbb` m2d notes,
  `dc209fb` m2c fan-out/compare); Xvfb smoke clean.

## Next up

1. Live Tier A check: run app against real `opencode serve`, create 2
   worktrees via `+ wt`, fan-out a prompt, annotate a diff line, merge —
   needs provider key for prompts (Tier B).
2. M3 scoping: SSH remote worktrees vs packaging vs usage tracking.
3. Optional: rename local `master` → `main`.

## Blockers

- None for build; live LLM verification needs a provider key.
