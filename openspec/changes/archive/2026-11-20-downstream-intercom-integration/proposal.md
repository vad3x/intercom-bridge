# Proposal: Downstream Intercom Bridge Integration & Local Patch Elimination

## Why

The `intercom-matter` workspace currently depends on a local path checkout of `rs-matter-embassy` containing local commits and uncommitted S31 `wifi-only` edits, plus `[patch.crates-io]` overrides for `rs-matter` and `rs-matter-stack`. Several local `rs-matter-embassy` patches are already superseded by upstream features (`#66` `enet-extra-sockets-*`, `#95` `NetTcpStack`, `#96` `with_protocols`) or can be handled in `intercom-firmware`'s C6 memory layout without touching `rs-matter-embassy/src/eth.rs` (`Bucket A`), while the remaining differences belong in 4 focused upstream PRs (`Bucket B`).

## What Changes

- **Bucket A (Eliminate Unneeded Local Patches Without Upstream PRs)**:
  - Revert the uncommitted S31 `wifi-only` working-tree edits in `rs-matter-embassy` and drop the superseded `Protocol::AX` patch (`cf19288`).
  - Drop custom `ENET_EXTRA_TCP_SOCKETS` / `ENET_EXTRA_UDP_SOCKETS` patches (`e2495cd`, `11d40d6`) in `rs-matter-embassy/src/enet.rs` and enable upstream `"enet-extra-sockets-4"` in `crates/intercom-adapter-matter/Cargo.toml`.
  - Drop the `OnceCell<Box<...>>` buffer pool patch (`11d40d6`, `1665e20`) in `rs-matter-embassy/src/eth.rs`, restoring upstream inline `EmbassyNetContext` buffers and rebalancing C6's `HEAP_SIZE` / `MatterStackStorage` placement so `.stack` preserves its >= 48 KiB budget and runtime free heap stays unchanged.
- **Bucket B (Consume Upstreamed `rs-matter` & `rs-matter-embassy` PRs)**:
  - Once the 4 upstream `rs-matter-embassy` PRs land (Wi-Fi scan config/filtering, `dhcpv4-hostname`, `mld-v1-compat` + `nd-learn-from-frames`, and `embassy_net::Stack` access), remove the local path dependency and `[patch.crates-io]` overrides.
  - Verify all host tests, C6/S3 release builds, C6 48 KiB `.stack` budget, and hardware smoke tests pass.

## Capabilities

### New Capabilities
- `downstream-integration`: Zero-local-patch integration of `rs-matter` and `rs-matter-embassy` with preserved ESP32-C6 48 KiB `.stack` and runtime heap budgets.

### Modified Capabilities
<!-- None -->

## Impact

- **Affected Repositories**:
  - `intercom-matter` (`Cargo.toml`, `crates/intercom-adapter-matter/Cargo.toml`, `crates/intercom-firmware/src/config.rs`, `crates/intercom-firmware/src/platform/c6/`)
  - `rs-matter-embassy` (reverting uncommitted S31 `wifi-only` WIP and dropping Bucket A commits)
- **API Surface**: None (internal workspace dependency and memory layout configuration).
- **Breaking Changes**: None.
