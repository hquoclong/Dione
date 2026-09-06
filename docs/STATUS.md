# ADE Status (living file — update at the end of every task)

## Current milestone

M2 — Fleet multi-agent DONE, Tier A live PASSED. Ready for M3.
Next: M3a `transcript.rs` (UnifiedMessage/Cost + dual-write mirror).

## Last commit (đã verify)

- `bed2d72` feat(m2f): Tier A live test (`tests/live_tier_a.rs`, 196 dòng)
  — serve thật + 2 worktrees + sessions + diff fetch + merge winner +
  remove, 2 passed in ~9s, không tốn prompt
- `41de1a5` docs: v2 plan M3–M15 + specs + ADRs + labs (13 files)
- Verified: check pass, clippy 0 warnings (ngoài future-incompat của
  dep `proc-macro-error2`), 26/26 unit + Tier A xanh, `cargo fmt` sạch,
  Xvfb smoke clean (app sống 60s, không panic).
- Local `master` → `main` (track `origin/main`); dọn worktree `task-1` thừa.
- Chưa push: local ahead `origin/main` 2 commits (`41de1a5`, `bed2d72`).

## Next up

1. M3a `transcript.rs`: `UnifiedMessage`/`Cost` + Store dual-write + test
   mirror (theo `ARCHITECTURE-v2.md`, <250 dòng).
2. Tier B live prompt: opt-in `ADE_LIVE_PROMPT=1 cargo test -p ade-core
   --features integration-tests tier_b` — tốn quota, chạy tay khi cần.

## Blockers

- None. Tier B cần user duyệt chi quota (keys đã có sẵn trong env).
