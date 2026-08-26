# ADE — Agent Development Environment

Native desktop app (Linux, Rust, **GPUI**) để chạy và soi bên trong AI agent:
quản lý session, stream timeline realtime, inspect context window, phê duyệt
permission, xem diff. Agent engine là `opencode serve`, điều khiển qua Rust SDK
[`opencode-codes`](https://docs.rs/opencode-codes).

## Kiến trúc

```
crates/
├── ade-core/          # runtime layer (lib, headless-testable) — KHÔNG phụ thuộc UI
│   ├── config.rs      # AppConfig (~/.config/ade/config.toml)
│   ├── server.rs      # AdeServer: spawn/quản lý `opencode serve` + client
│   ├── state.rs       # Store: state mirror, apply_event(), reconcile
│   ├── runtime.rs     # vòng lặp tokio (thread riêng): commands + SSE pump + poll
│   └── context.rs     # compiler dựng view-model context window + ước lượng token
└── ade-ui/            # GPUI desktop app (bin)
    └── src/app.rs     # AdeApp root view: top bar / sidebar / timeline /
                       # composer / right panel (Context|Inspector|Diff) / permission overlay
```

Luồng dữ liệu: UI thread → `Command` qua unbounded channel → runtime loop
(thread riêng, tokio 2 workers) → mutate `Store` → publish snapshot `Arc<Store>`
qua `RwLock` → GPUI view poll mỗi 160ms (`cx.spawn` + timer), chỉ `cx.notify()`
khi Arc ptr đổi.

## Lệnh phát triển

```bash
cargo check --workspace                  # typecheck
cargo clippy --workspace --all-targets   # lint (0 warning; future-incompat của proc-macro-error2 là transitive dep, bỏ qua)
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
# Smoke test không display (GPUI chọn Wayland nếu thấy WAYLAND_DISPLAY → phải unset):
env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET xvfb-run -a -s "-screen 0 1440x900x24" ./target/debug/ade-ui
```

Toolchain: edition 2024, MSRV khai báo `rust-version = "1.90"` (chưa verify
bằng toolchain 1.90 thật). Rust cài qua rustup; PATH persist trong
`/etc/sandbox-persistent.sh`. Package hệ thống: libxkbcommon(-x11),
libwayland, X11 client libs, xvfb, mesa-vulkan-drivers.

## Quy ước & bẫy API (đã mất thời gian phát hiện)

- **egui đã bị thay thế bằng GPUI** — không còn SidePanel/eframe. Nếu cần tham
  chiếu bản cũ: git tag/commit `7755385`.
- **gpui 0.2.2 (crates.io)**: dùng THUẦN crates.io, đừng trộn git dep của zed
  main → xung đột kiểu. Platform features (`font-kit`,`wayland`,`x11`) là
  default; Linux render qua blade/Vulkan.
- GPUI bootstrap pattern chuẩn: `Application::new().with_assets(..).run(|cx|
  { ...; cx.open_window(opts, |window, cx| { cx.new(|cx| Root::new(AnyView::from(v), window, cx)) }) })`.
  Cần `use gpui::AppContext as _` để có `.new()` trên `&mut App`.
- `overflow_y_scroll()` chỉ có trên element đã `.id(...)` (Stateful).
- `rgba(u32)` KHÔNG phải const fn — bọc trong helper fn, không dùng trong
  `const`. `.whitespace_pre()` không tồn tại trên Div — render JSON theo dòng.
- `WeakEntity::read_with(cx, |app, cx_app| ...)` closure nhận 2 args.
- `cx.listener(...)` trả về Fn nên bên trong phải `.clone()` biến captured,
  không move.
- **gpui-component 0.5.1**: `Label` ở `gpui_component::label::`; Button không
  có `.ghost()` — dùng `.outline()`; `TextView::markdown(id, md, window, cx)`
  cần window+App (đặt id unique cho mỗi markdown block);
  `InputState::new(window, cx)` + builder `.auto_grow/.placeholder`;
  events qua `cx.subscribe_in(&input, window, |this,_,ev,window,cx|)`,
  `InputEvent::PressEnter { .. }`; `Input::new(&entity_input_state)`.
- **opencode-codes**: SSE best-effort — LUÔN reconcile bằng REST
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
  tests/context_tests.rs) — nhanh, deterministic.
- Integration tests spawn server thật (tests/live.rs), chia tier: Tier A không
  cần LLM; Tier B gate bởi env `ADE_LIVE_PROMPT=1`.
- Smoke test GUI: script Xvfb kiểm tra cửa sổ mở qua `xwininfo`.

## Hướng phát triển tiếp

Sửa memory blocks trực tiếp, multi-session song song, resizable panels
(gpui-component `resizable` module), tracing subscriber thực sự (hiện log
tracing bị drop), packaging (AppImage/deb), release build tối ưu size.
