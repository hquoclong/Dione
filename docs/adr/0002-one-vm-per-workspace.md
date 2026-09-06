# ADR 0002: 1 VM / 1 workspace (không phải 1 VM / worktree)

Ngày: 2026-09-05. Trạng thái: chấp nhận.

## Ngữ cảnh

Mỗi task = 1 worktree cô lập. Hỏi: mỗi worktree 1 VM (cách ly mạnh nhất)
hay gom worktrees vào 1 VM của workspace?

## Quyết định

**1 VM / 1 workspace.** Worktrees nằm trong mount virtiofs của VM đó.

## Vì sao

- RAM/CPU: N worktrees × N VM tốn kém cho solo dev chạy 2–5 agents.
- Mô hình Docker `sbx`: 1 sandbox = 1 VM = 1 workspace mount.
- Cách ly giữa tasks vẫn đủ nhờ git worktree (branch + thư mục riêng);
  VM lo cách ly với host (mối đe dọa chính: agent phá máy).

## Hệ quả

- `VmManager` key theo workspace (canonical repo path).
- Worktree create/remove là thao tác git trong mount, không boot VM mới.
- Muốn cách ly mạnh hơn (task untrusted hoàn toàn) → để backlog M14+
  (opt-in 1 VM/task).
