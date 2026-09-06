# ADR 0003: Repo trên host là source of truth, mount virtiofs read-write

Ngày: 2026-09-05. Trạng thái: chấp nhận.

## Ngữ cảnh

Code sống ở đâu: host hay VM? Sync kiểu gì (mount live vs git push/pull)?

## Quyết định

**Host giữ repo; VM mount read-write qua virtiofs.** `git_diff`/`merge`
chạy trên mount nên host thấy ngay, không sync tay.

## Vì sao không push/pull

- Push/pull mỗi lần review chậm + dễ quên + conflict giả.
- Học `sbx` filesystem passthrough: sync 2 chiều tức thì, `git status`
  nhanh nhờ cache (có cờ opt-out `VIRTIOFS_CACHE=0`).

## Hệ quả

- Merge winner trong VM hiện ngay trên host.
- Lab bắt buộc: `touch` 2 chiều trước khi chạy agent.
- Fallback mount hỏng → báo lỗi rõ (push/pull qua SSH để M sau).
