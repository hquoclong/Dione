# ADE — Agent Development Environment

Native desktop app (Linux, Rust) để chạy và soi bên trong AI agent: quản lý
session, stream timeline realtime, inspect context window, phê duyệt permission,
xem diff. Agent engine là `opencode serve`, điều khiển qua Rust SDK
[`opencode-codes`](https://docs.rs/opencode-codes).

## Kiến trúc

```
crates/
├── ade-core/          # runtime layer (lib, headless-testable)
│   ├── config.rs      # AppConfig (~/.config/ade/config.toml)
│   ├── server.rs      # AdeServer: spawn/quản lý `opencode serve` + client
│   ├── state.rs       # Store: state mirror, apply_event(), reconcile
│   ├── runtime.rs     # vòng lặp tokio: commands + SSE pump + poll safety net
│   └── context.rs     # compiler dựng view-model context window + ước lượng token
└── ade-ui/            # egui desktop app (bin)
    └── src/app/       # app.rs shell + timeline/context_panel/inspector/diff_view
```

Luồng dữ liệu: UI thread → `Command` qua unbounded channel → runtime loop
(thread riêng, tokio 2 workers) → mutate `Store` → publish snapshot
`Arc<Store>` qua `RwLock` → UI clone Arc mỗi frame.

## Lệnh phát triển

```bash
cargo check --workspace                  # typecheck
cargo clippy --workspace --all-targets   # lint (phải sạch 0 warning)
cargo fmt --all                          # format trước khi commit
cargo test -p ade-core                   # unit tests (10 tests, không cần mạng)

# Live tests (spawn `opencode serve` thật):
cargo test -p ade-core --features integration-tests

# Tier B — prompt thật tới LLM (cần provider key):
ADE_LIVE_PROMPT=1 cargo test -p ade-core --features integration-tests tier_b
```

Chạy app:

```bash
cargo run -p ade-ui                      # desktop bình thường
# Smoke test không display:
env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET xvfb-run -a ./target/debug/ade-ui
```

Lưu ý sandbox: cần `WINIT_UNIX_BACKEND=x11` hoặc unset `WAYLAND_DISPLAY`;
package hệ thống đã cài: libxkbcommon(-x11), libwayland, X11 client libs, xvfb.
Rust được cài qua rustup; PATH đã persist trong `/etc/sandbox-persistent.sh`.

## Quy ước & bẫy API (đã mất thời gian phát hiện)

- **egui 0.36**: `SidePanel`/`TopBottomPanel` KHÔNG còn — dùng
  `egui::Panel::left/right/top/bottom` với `.default_size()` (không phải
  `.default_width()`). Trait `eframe::App` yêu cầu `fn ui(&mut self, ui: &mut
  Ui, frame)` thay vì `update(ctx, frame)` — panel nhận `&mut Ui`, không nhận
  `&Context`. `RichText::italics()` (có s).
- **egui_commonmark**: phải dùng 0.25 cho egui 0.36 (0.21 kéo egui 0.32 về →
  xung đột kiểu `Ui`).
- **opencode-codes**: SSE là best-effort — LUÔN reconcile bằng REST
  (`list_messages`) theo chu kỳ và sau `StreamEvent::Connected`.
  `StreamEvent`/`Event` là `#[non_exhaustive]` → match cần wildcard arm.
- Wire shape dễ sai: `SessionStatus` là enum tag `"type"`
  (`{"type":"busy"}`); event envelope có `properties` (không phải `data`)
  trừ `MessagePartUpdated.properties.data.part`. `SessionCreateParamsModel`
  dùng field `id`; `SubtaskPartInputModel` dùng `model_id`.
- Không re-export `portpicker` từ SDK — khai báo dependency trực tiếp.
- reqwest phải khớp features của SDK (`rustls-tls`, default-features=false)
  để tránh dual TLS stacks.

## Testing approach

- Unit tests mô phỏng SSE events bằng JSON wire thật (tests/state_tests.rs,
  tests/context_tests.rs) — chạy nhanh, deterministic.
- Integration tests spawn server thật (tests/live.rs), chia tier: Tier A không
  cần LLM; Tier B gate bởi env `ADE_LIVE_PROMPT=1`.

## Hướng phát triển tiếp

Sửa memory blocks trực tiếp, multi-session song song, git worktree per task,
MCP tool wiring UI, tracing/token analytics, packaging (AppImage/deb).
