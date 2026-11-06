# Proposal: Downstream Intercom Bridge Integration & Verification

## Why

Once upstream PRs land in `project-chip/rs-matter` (Audio Doorbell cluster and Matter-over-TCP) and `sysgrok/rs-matter-embassy` (Wi-Fi 6 and on-demand buffers), the `intercom-matter` workspace must be updated to consume these upstream releases, remove fragile local path patches, and verify full system stability on target hardware.

## What Changes

- Update `Cargo.toml` in `intercom-matter` to point to upstream branches/releases, removing local `[patch.crates-io]` entries.
- Verify all 107 host tests pass across `intercom-core`, `intercom-adapter-http`, and `intercom-pairing-tool`.
- Verify release DevKit firmware builds cleanly within the 48 KiB execution stack allocation.

## Capabilities

### New Capabilities
- `downstream-integration`: Clean integration of upstreamed Matter standard crates with end-to-end verification.

### Modified Capabilities
<!-- None -->

## Impact

- **Affected Repositories**:
  - `intercom-matter` (`Cargo.toml`, `crates/intercom-firmware`)
- **API Surface**: None (internal workspace configuration).
- **Breaking Changes**: None.
