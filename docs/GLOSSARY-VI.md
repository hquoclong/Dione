# Từ vựng ADE (tiếng Việt, ví dụ đời thường)

Dành cho người không phải kỹ sư phần mềm. Đọc 1 lần, tra cứu khi gặp từ lạ.

## Host — máy bạn đang ngồi

Máy vật lý chạy ADE UI. Giống như "nhà chính". Mặc định app chạy ở đây
(terminal, SSH client, secrets). An toàn vì agent chưa chạy ở đây.

## MicroVM — căn phòng cách ly trong nhà

Máy ảo siêu nhẹ: có **kernel riêng** (não riêng), boot <1s, tốn vài MB RAM.
Agent chạy trong phòng này; có phá đồ cũng không lan ra nhà chính.
Khác container (container share kernel = share não với host).

- Ví dụ: Docker Sandboxes (`sbx`) mỗi sandbox là 1 microVM.
- ADE dùng **Cloud Hypervisor** (VMM viết bằng Rust, có REST API).

## KVM — chìa khóa vào phòng

`/dev/kvm` là module kernel Linux cho phép tạo VM bằng phần cứng
(Intel VT-x / AMD-V). Không có KVM → ADE tự rớt về Host mode + banner
"VM unavailable", không crash. Kiểm tra: `ls -l /dev/kvm`.

## virtiofs — cửa sổ lùa giữa nhà và phòng

Chia sẻ thư mục host ↔ VM 2 chiều tức thì. Repo trên host mount
read-write vào VM; agent sửa trong VM là host thấy ngay.
Tắt cache khi `git status` lạ: `VIRTIOFS_CACHE=0`.

## vsock — ống nói chuyện host ↔ VM

Kênh socket riêng host-guest, không qua mạng ngoài. Dùng để SSH vào VM,
forward port preview. Nhanh + không lộ ra LAN.

## SSH — chìa khóa + ống nói

`ssh -i <key-ephemeral> vm@...` để mở terminal vào VM. Key sinh mới mỗi
lần boot, không reuse. `~/.ssh/authorized_keys` trong VM được bơm lúc boot.

## Worktree — bàn làm việc riêng

`git worktree add` checkout 1 branch ra 1 thư mục riêng.
ADE: `<repo>/.ade-worktrees/<slug>` + branch `ade/<slug>`.
1 task = 1 worktree = N agent chạy song song không giẫm file nhau.

## Workspace — cả tầng làm việc

1 repo + cấu hình + worktrees. ADE: **1 VM / 1 workspace**
(worktrees nằm trong mount của VM, tiết kiệm RAM hơn 1 VM/worktree).

## Agent — người thợ trong phòng

CLI bất kỳ chạy trong terminal (Claude Code, Codex, OpenCode, Gemini…).
ADE không bundle agent, không khóa agent mặc định (BYO).
Thêm agent mới = thêm 1 kit script (xem `AGENT-ANY.md`).

## Kit script — công thức lắp thợ

Script cài agent lúc boot VM (`kits/claude.sh`…). Image gốc minimal,
agent luôn mới nhất mà không build lại image. Học từ Docker `sbx` kits.

## Backend/Provider — ổ cắm thay được

Interface (trait) + nhiều implementation: `VmBackend { CloudHypervisor,
ExternalSbx, Mock }`, `WorkspaceProvider { Host, MicroVm }`,
`AgentBackend { Opencode, Terminal }`. Thay ổ cắm không đụng tường (legacy).

## Transcript — biên bản cuộc họp

`UnifiedMessage { role, text, tool, ts }` thay cho `Message/Part` của
opencode trong UI. Mọi agent đều dịch về 1 biên bản chung để Chat render.

## Secrets proxy — két sắt ở nhà chính

API key nằm ở keychain host. Khi agent cần, host bơm vào env của lệnh
exec, không ghi file trong VM. Agent gọi được API nhưng không đọc được key.

## NetPolicy — nội quy mạng

`Open` (cho hết, p1) / `Balanced` (chặn + allowlist) / `Locked` (chặn hết).
Code chừa sẵn enum, p1 chạy Open.

## Dispatcher/kanban — quản đốc

Vòng lặp mỗi 60s: thu hồi task kẹt (stale/crash), giao việc, retry có giới
hạn (`failure_limit=2 → blocked`). Học Hermes kanban.

## Factory — dây chuyền

`factory.yaml` as-code: triggers/agents/gates check-in vào repo.
Học Warp Factories. Đo $/PR, evals, benchmarks.
