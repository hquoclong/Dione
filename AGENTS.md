# ADE — Agentic IDE

Native Linux desktop app (Rust + GPUI) to run and inspect AI coding agents.
One task = one isolated git worktree, many agents in parallel, inline diff
review with notes sent back to the agent. Agent engine is `opencode serve`,
driven via the Rust SDK `opencode-codes`.

Status: fresh restart at `02c1b89` (legacy MVP dropped). No canvas — that
lives in a separate project. Focus: M0 bootstrap, M1 single-agent, M2 fleet.

## Stack

- Rust 2024 edition, MSRV `rust-version = "1.90"`, workspace resolver 3.
- `gpui = "=0.2.2"` from crates.io only — never mix with zed git deps
  (type conflicts). Linux renders via blade/Vulkan.
- `gpui-component = "=0.5.1"` + `gpui-component-assets` (Label, Button,
  Input, TextView markdown, dock/resizable, tables).
- `opencode-codes 1.18` (`async-client`, `server`), tokio multi-thread,
  reqwest `rustls-tls` with `default-features = false` (avoid dual TLS).
- Install Rust via rustup; system deps: build-essential, libxkbcommon(-x11),
  libwayland, X11 client libs, xvfb, mesa-vulkan-drivers.

## GPUI ecosystem (why GPUI)

Lightweight + GPU-accelerated (120 FPS target), small binaries (~12 MB min),
native feel. Key community resources:

- `longbridge/gpui-component` (13k+ stars): 60+ components, dock layout with
  resizable panels/tabs/splits, virtual tables/lists, code editor stub with
  Tree-sitter + LSP hooks, markdown/HTML, charts, themes. Has AI skills:
  `npx skills add longbridge/gpui-component` (`gpui` + `gpui-component`).
- `gpui-ce/gpui-ce`: community edition fork, API-compatible, stability focus.
- `edo-zhou/awesome-gpui`: curated list of crates/examples/tools.
- Upstream learning: Zed source (`crates/gpui`), Zed Discord, `gpui.rs`.
- This project pins crates.io `gpui 0.2.2`; the main branch of
  `longbridge/gpui-component` tracks zed git — check version compat before
  upgrading (0.5.1 matches our pin).

## Commands

```bash
cargo check --workspace
cargo clippy --workspace --all-targets   # 0 warnings
cargo fmt --all                          # before every commit
cargo test -p ade-core                   # unit, no network

# Live (spawns real `opencode serve`):
cargo test -p ade-core --features integration-tests
ADE_LIVE_PROMPT=1 cargo test -p ade-core --features integration-tests tier_b

cargo run -p ade-ui
# Headless smoke (GPUI picks Wayland if WAYLAND_DISPLAY is set — unset it):
env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET xvfb-run -a -s "-screen 0 1440x900x24" ./target/debug/ade-ui
```

## Architecture

```
crates/
├── ade-core/          # runtime, headless-testable — NO UI dependency
│   ├── config.rs      # AppConfig (~/.config/ade/config.toml)
│   ├── worktree.rs    # pure helpers: slug/branch/path + WorktreeRecord
│   ├── state.rs       # Store mirror: sessions/statuses/messages/diffs/worktrees
│   ├── server.rs      # (M1) spawn/manage `opencode serve` + client [planned]
│   ├── runtime.rs     # (M1) tokio loop: commands + SSE pump + poll [planned]
│   └── context.rs     # (M1) context-window view-model + token est [planned]
└── ade-ui/            # GPUI desktop bin
    └── src/{main,app}.rs  # Root view: topbar/sidebar/timeline/composer/right panel
```

Data flow: UI thread → `Command` channel → runtime thread → mutate `Store` →
publish `Arc<Store>` via `RwLock` → GPUI view polls every 160 ms, `cx.notify()`
only when the Arc ptr changes.

## Worktree conventions (M2 core, cf. Orca/Hermes/Codex App)

- Path: `<repo>/.ade-worktrees/<slug>` (e.g. `feat-auth-3f2a`), branch `ade/<slug>`.
- One branch in one worktree (hard git rule). Detached HEAD for experiments,
  named branch to keep. Max ~15 managed worktrees.
- `.gitignore` files do NOT transfer — use `.worktreeinclude` to copy `.env`/venvs.
- Each worktree needs its own dep install. Auto-cleanup on exit, prune stale on
  startup, keep dirty worktrees for manual recovery.
- Sidebar groups `repo → worktree → sessions`; dashboard `Needs you/Working/Done`.

## GPUI + wire pitfalls

- Bootstrap: `Application::new().with_assets(..).run(|cx| { gpui_component::init(cx);
  cx.open_window(opts, |w, cx| cx.new(|cx| Root::new(AnyView::from(v), w, cx))) })`.
  Need `use gpui::AppContext as _` for `.new()` on `&mut App`.
- `overflow_y_scroll()` only on `.id(...)` elements; `rgba(u32)` is NOT const —
  wrap in helper fns; no `.whitespace_pre()` on Div.
- `WeakEntity::read_with(cx, |app, cx_app| ...)` takes 2 args; `cx.listener`
  returns Fn so `.clone()` captures inside.
- gpui-component: `Label` at `gpui_component::label::`; Button has `.outline()`
  (no `.ghost()`); `TextView::markdown(id, md, window, cx)` needs unique id per
  block; `InputState::new(window, cx)` + `InputEvent::PressEnter`.
- SSE is best-effort — always reconcile via REST (`list_messages`) on interval
  and after `StreamEvent::Connected`. `StreamEvent`/`Event` are
  `#[non_exhaustive]` → wildcard arm required.
- Wire: `SessionStatus` is `{"type":"busy"}`; envelope uses `properties` (not
  `data`); `portpicker` is NOT re-exported by the SDK — depend directly.

## Workflow

- Plan mode for design, build mode for code. Small PRs (`feat/*`, <300 lines).
- Solo learn-build loop: research 30% → slice <3 days → `cargo test` + demo.
