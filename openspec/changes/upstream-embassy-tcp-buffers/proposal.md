# Proposal: Upstream On-Demand TCP Buffers in rs-matter-embassy

## Why

Static reservation of TCP socket buffers for Matter port 5540 consumes 16–24 KiB of SRAM continuously, even when TCP is idle. On resource-constrained SoCs such as ESP32-C6 (320 KiB usable RAM), this static footprint starves the execution stack and Opus audio encoder heap. Allocating TCP buffers dynamically on connection start and returning them on teardown preserves critical memory headroom.

## What Changes

- In `rs-matter-embassy/src/eth.rs` and `rs-matter-embassy/src/enet.rs`:
  - Wrap `EnetMatterTcpBuffers` in `OnceCell<Box<EnetMatterTcpBuffers>>` (or dynamic pool) so buffers allocate only when TCP bind/accept occurs.
  - Implement teardown hooks to reclaim buffers when connections close.
  - Wire into `EnetStack` to satisfy the TCP accept trait introduced in `rs-matter` PR 2.

## Capabilities

### New Capabilities
- `embassy-on-demand-tcp-buffers`: Dynamic on-demand lifecycle for Matter-over-TCP socket buffers in `rs-matter-embassy`.

### Modified Capabilities
<!-- None -->

## Impact

- **Affected Repositories**:
  - `sysgrok/rs-matter-embassy` (`rs-matter-embassy/src/eth.rs`, `rs-matter-embassy/src/enet.rs`)
- **API Surface**:
  - `EmbassyNetContext::tcp_buffers()` provides dynamic access to TCP buffer storage.
- **Breaking Changes**: None. Internal allocation optimization.
