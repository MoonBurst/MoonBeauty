# Split `backend/system.rs` into a folder

**Effort:** 3–4h
**Subsystem:** hardware
**Depends on:** —
**Good first issue:** no

## Current state

`src/libraries/backend/system.rs` is ~1,286 lines. Contains
`SystemCollector` state and per-domain collectors: CPU, GPU (incl.
NVIDIA paths + name resolution), RAM, net, battery. Plus all the
metric structs.

## Desired state

    src/libraries/backend/system/
    ├── mod.rs           SystemCollector struct + public re-exports
    ├── types.rs         metric structs + ProcessItem
    ├── cpu.rs
    ├── gpu.rs
    ├── ram.rs
    ├── net.rs
    └── battery.rs

`mod.rs` owns the shared `SystemCollector` state (prev_bandwidth,
prev_rx_bytes, prev_net_time, etc.) so per-domain impls can call into
it.

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing)
- Manual: open CPU / GPU / RAM capsules in the bar; verify tooltips
  show correct values and process lists populate. Kill a process from
  the CPU tooltip and verify it dies.
