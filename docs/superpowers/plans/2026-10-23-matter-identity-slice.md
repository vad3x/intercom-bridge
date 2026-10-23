# Matter Identity and Runtime Wrapper Slice

**Goal:** Replace the remaining duplicated Matter identity, readiness, storage,
and connection policy with the local `edf-matter` foundation at revision
`1011623`, preserving device behavior and public Intercom composition paths.

**Architecture:** Matter-specific policy moves out of `intercom-core` and into
`intercom-adapter-matter`. The firmware composes the same foundation identity
used by `intercom-pairing-tool`, so commissioning credentials cannot drift
between the CLI and the device. Product-owned endpoint metadata, ecosystem
reporting, cluster handlers, and `UserTask` wiring stay in Intercom.

## Constraints

- Keep `IC` serial prefix, `intercom-bridge` hostname prefix, VID `0xfff1`,
  PID `0x8001`, endpoint IDs, lifecycle variants, and pairing vectors.
- Keep the local permissive CLI MAC parser; do not tighten accepted syntax.
- Do not add the foundation OTA error yet; OTA remains a later slice.
- No hardware, flashing, OTA upload, private-key access, or publishing.
- Verify host packages, firmware host library, partitioned KV tests, strict
  core Clippy, scoped formatting, and the RISC-V release build.

## Tasks

### Task 1: Converge commissioning identity

- Add `edf-matter` to the workspace and adapter dependencies.
- Remove `intercom-core/src/domain/commissioning.rs` and its exports.
- Wrap `edf_matter::identity::derive_device_identity` in the Matter adapter with
  Intercom prefixes and use it in `MatterController::run`.
- Remove duplicate serial/hostname formatting from adapter `config.rs`.
- Add device-identity reference tests.

### Task 2: Wrap readiness policy

- Replace Intercom's duplicate readiness state machine with
  `edf_matter::readiness::ClusterReadinessBarrier` plus the existing Matter
  pointer and ecosystem reporting.

### Task 3: Reuse partitioned storage

- Replace the firmware-local `PartitionedKvBlobStore` implementation with a
  compatibility alias for `edf_matter::store::PartitionedWifiStore`.
- Keep the existing partitioned-KV integration tests as behavioral coverage.

### Task 4: Wrap connection policy

- Replace local Apple Hub exclusion and connection transition logic with
  `edf_matter::connection::{FabricPolicy, ConnectionState}`.
- Keep the Intercom `UserTask` adapter and startup orphan reset behavior.

### Task 5: Cross-check CLI and device identity

- Add colon, hyphen, and compact MAC characterization tests to the pairing CLI.
- Compare CLI output with `edf_matter::identity` across those formats.
- Update `docs/local-foundation-integration.md` with evidence and remaining
  scope.

## Verification

Use `mise exec -- cargo ... --target x86_64-apple-darwin --locked --offline`
for host checks. Use the RISC-V target for the firmware release build. Record
test totals and any API deltas in the integration document.
