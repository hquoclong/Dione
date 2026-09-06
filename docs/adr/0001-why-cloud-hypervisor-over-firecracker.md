# ADR 0001: Cloud Hypervisor thay vì Firecracker thuần

Ngày: 2026-09-05. Trạng thái: chấp nhận.

## Ngữ cảnh

Cần 1 VMM Rust nhẹ/nhanh/ổn định cho 1 VM/workspace trên Linux, học Docker
`sbx`. Ứng viên: Firecracker, Cloud Hypervisor, QEMU, libkrun, Kata/gVisor.

## Quyết định

**Cloud Hypervisor** làm backend chính; `ExternalSbx` (gọi `sbx` CLI) làm
đường tắt; `Mock` cho CI.

## Vì sao không chọn cái kia

- Firecracker (~125ms boot, <5MiB): nhưng **không có virtiofs
  share-folder, không GPU**, tự lo kernel/rootfs/network rất nặng cho
  solo dev.
- QEMU (~2M dòng C): nặng, attack surface lớn.
- libkrun: nhúng dạng library còn non cho desktop.
- Kata/gVisor: tiện container nhưng cách ly yếu hơn / I/O overhead.

Cloud Hypervisor: cùng họ rust-vmm, có **virtiofs + REST API + hotplug**,
boot <1s — đúng thứ workspace dev cần (mount repo host vào VM).

## Hệ quả

- M5 implement `CloudHypervisorBackend` (boot→ssh→mount→stop).
- M đầu dùng image sẵn + `ExternalSbx` trước, tự build image sau.
- Build `host-only` không cần KVM vẫn qua.
