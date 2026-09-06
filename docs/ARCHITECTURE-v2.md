# ADE Architecture v2 (agent-agnostic + Workspace/MicroVM)

> V1 (`ARCHITECTURE.md`) mô tả M1–M2 opencode-coupled. V2 là mục tiêu
> M3→M15. V1 giữ nguyên để tra cứu legacy; code mới bám V2.

## Sơ đồ lớn

```
┌─ Host (mặc định, Warp-like) ─────────────────────────┐
│ ADE UI (GPUI) + local terminal (pty) + SSH client    │
│ secrets ở keychain host, chưa chạy agent ở đây       │
└───────────────────────┬──────────────────────────────┘
                        │ chỉ khi Open Workspace / Run Agent
┌─ Workspace (1 repo) ──▼──────────────────────────────┐
│ 1 MicroVM / 1 workspace                              │
│ ├── worktrees trong mount virtiofs                   │
│ │    <repo>/.ade-worktrees/<slug> (branch ade/<slug>)│
│ ├── SSH server (key ephemeral mỗi boot)              │
│ ├── guest-agent nhỏ (exec / heartbeat)               │
│ └── agent CLI bất kỳ (kit script lúc boot)           │
└──────────────────────────────────────────────────────┘
```

- Source of truth = **repo trên host**, mount read-write vào VM qua
  virtiofs. Merge trong VM hiện ngay trên host.
- Không `/dev/kvm` → fallback Host mode + banner, không crash.

## Crates (mục tiêu, 1 chiều, không vòng)

```
crates/
├── ade-core/        # FROZEN M1–M2: Store/Command cũ giữ nguyên (dual-write)
│   └── + modules mới tạm trú: transcript.rs → agent.rs → workspace.rs → vm.rs
├── ade-workspace/   # (tách ở M5) Workspace + Task + WorkspaceProvider { Host, MicroVm }
├── ade-vm/          # (tách ở M5) VmManager + VmBackend { CloudHypervisor, ExternalSbx, Mock }
├── ade-agent/       # AgentBackend { OpencodeAdapter, TerminalAdapter } + kits/*.sh
└── ade-ui/          # HostShell + WorktreeView [Chat | Terminal]
```

Quy tắc chống vỡ legacy:

1. Không sửa signature `Store`/`Command` cũ — chỉ **thêm** types mới.
2. Module mới sống trong `ade-core` ở M3–M4 (<250 dòng/file), tách crate
   khi API ổn định (M5).
3. Mọi backend có `Mock` để CI/Xvfb không KVM vẫn xanh.
4. Build mặc định `host-only` không cần KVM.

## Core types (khóa để review)

```rust
// transcript: biên bản chung cho mọi agent
enum Role { User, Agent, Tool }
struct UnifiedMessage { id, task: TaskId, role: Role, text, tool: Option<ToolCall>, ts: u64 }
struct Cost { input, output, cache, cost: f64 }

// agent: ổ cắm thay được, không khóa opencode
trait AgentBackend: Send {
    fn spawn(&mut self, ws: &dyn WorkspaceProvider, prompt: &str) -> Result<SessionId>;
    fn prompt(&mut self, s: &SessionId, text: &str) -> Result<()>;
    fn abort(&mut self, s: &SessionId) -> Result<()>;
    fn poll(&mut self) -> Vec<AgentEvent>;
}
enum AgentStatus { Idle, Working, NeedsInput{ reason: String }, Done, Error{ msg: String } }

// workspace: Host và VM chung 1 mặt
trait WorkspaceProvider: Send {
    fn exec(&mut self, cmd: &[&str], cwd: &Path) -> Result<ExecOut>;
    fn shell(&mut self) -> Result<ShellChannel>;
    fn ssh_info(&self) -> Option<SshInfo>; // None = Host
    fn git_diff(&self) -> Result<GitDiff>; // qua git, không qua /session/diff
}

// vm: 1 VM / 1 workspace
enum VmState { Missing, PullingImage, Booting, WaitingSsh, Mounting, Ready, Running, Stopped }
enum NetPolicy { Open, Balanced, Locked } // p1 = Open, chừa hook
trait VmBackend: Send {
    fn boot(&mut self, cfg: &VmConfig) -> Result<VmHandle>;
    fn wait_ssh(&self, h: &VmHandle) -> Result<SshInfo>;
    fn stop(&mut self, h: &VmHandle) -> Result<()>;
}
```

- `OpencodeAdapter` bọc nguyên `server.rs` + SSE/poll hiện tại.
- `TerminalAdapter` dùng `portable-pty`, status heuristic + nút `Mark done`.
- Agent không biết mình ở Host hay VM (chỉ thấy `WorkspaceProvider`).

## Luồng chính

**Boot workspace:** `Open` → probe `/dev/kvm` → pull image (nếu thiếu) →
boot → wait SSH → mount virtiofs → Ready → mở Terminal tab.
**Chạy agent:** `Run` → `VmManager` đảm bảo Ready → kit script cài agent
(nếu thiếu) → `AgentBackend::spawn(ws, prompt)` → poll `AgentEvent` →
dịch về `UnifiedMessage` → Chat render.
**Review:** `git_diff` qua mount → compare/cherry-pick → merge winner
(`--no-ff`) → prune. Secrets bơm qua env lúc exec, không ghi đĩa VM.
**Rớt mạng/KVM:** timeout SSH → `Error` + banner, giữ worktree để retry tay.

## Dữ liệu (Store dual-write 2 milestone)

`Store` cũ (`sessions: Session`, `messages: Message/Part`) giữ nguyên;
thêm `transcripts: Map<TaskId, Vec<UnifiedMessage>>` + `Cost`.
UI mới chỉ đọc `transcripts`. `apply_event(&Event)` thu hẹp thành
`apply_agent_event(&AgentEvent)` ở adapter. Xóa types opencode khỏi
`app.rs`/`context.rs` ở M3d (sau khi mirror đủ).
