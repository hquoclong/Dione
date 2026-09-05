# ADE Roadmap

## M0 — Bootstrap ✅ done (`7d8f4e1`)

Workspace, AGENTS.md, `ade-core` worktree/state pure logic, `ade-ui`
window skeleton opening under Xvfb.

## M1 — Single-agent ✅ done (`5bd2a94`, `66ec918`)

`opencode serve` spawn + client, Store mirror, SSE pump + poll reconcile,
context view-model, full UI (sidebar/timeline/composer/right
panel/permission gate/model picker). 15 tests, clippy 0 warnings.

## M2 — Fleet multi-agent ⬅ CURRENT

One task = one isolated git worktree, N agents in parallel.

- [x] `worktree.rs`: real git ops — create (`git worktree add`), list,
      remove, prune stale, `.worktreeinclude` copy (`495e044`)
- [x] 1 opencode session per worktree (session ↔ worktree link in Store,
      per-directory clients + SSE pumps) (`90c8a09`)
- [x] Dashboard: `Needs you / Working / Done` in grouped Fleet sidebar
- [x] Fan-out: 1 prompt → N worktrees (`⇉ all`); grouped multi-session
      diff compare (`↻ all`)
- [x] Annotate diff lines → send batch back to the right agent
- [x] Merge winner (`--no-ff`) + prune; keep dirty worktrees for manual
      recovery
- [x] Cap ~15 managed worktrees (enforced in `worktree::create`)

Conventions: path `<repo>/.ade-worktrees/<slug>`, branch `ade/<slug>`;
one branch in one worktree; detached HEAD for experiments.
Gitignore `.ade-worktrees/` in every target repo (else `git add .` warns
about embedded repos).

## M3+ — Later (not scheduled)

- SSH remote worktrees, mobile/Telegram ping on agent-done
- Usage/cost tracking per agent, automations/skills packs
- Packaging (AppImage/deb), release-size optimization
- Warp-style `factory.yaml` governance — inspiration only

## Out of scope

- Infinite canvas (separate project), code editing/LSP, model harness
  internals (memory, evals, guardrails).
