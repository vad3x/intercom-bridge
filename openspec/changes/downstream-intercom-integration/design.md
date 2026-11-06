# Design: Downstream Intercom Bridge Integration & Verification

## Context

The Intercom Bridge workspace currently patches `rs-matter`, `rs-matter-stack`, and `edge-nal-embassy` via local directory paths. Upstreaming these changes allows switching back to clean upstream tracking.

## Decisions

### 1. Phased Patch Removal
- **Decision**: Update `Cargo.toml` patch section sequentially as each upstream PR merges, verifying host tests at each step.
- **Rationale**: Isolates regressions between data model changes and transport runner changes.

### 2. DevKit SRAM Verification
- **Decision**: Validate that `HEAP_SIZE = 120 KiB` leaves at least 48 KiB for `.stack` without link-time or runtime stack overflow during Opus voiced encoding.
- **Rationale**: Guarantees zero regression on target ESP32-C6 DevKit hardware.

## Migration Plan

1. Point `Cargo.toml` to upstream PR branches/releases.
2. Run host test suite (`cargo test --target x86_64-apple-darwin`).
3. Build and test release DevKit firmware (`cargo build-devkit`).
4. Archive change with `openspec archive downstream-intercom-integration`.
