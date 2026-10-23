# Local Foundation Integration Stage A Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Begin Intercom consumption of the completed foundation without publishing it or changing device-facing behavior.

**Architecture:** Keep product policies, CLI UX, and device composition in Intercom. Replace shared implementation with local foundation dependencies and compatibility re-exports, one verified slice at a time. This stage covers pairing generation and source-compatible lifecycle/ecosystem/reset primitives; diagnostics, OTA, Matter, GPIO, dashboard and host scripts remain separate migration slices.

**Tech Stack:** Rust 1.99.1, Cargo through mise, Embassy 0.8 sync / 0.5.1 time, rs-matter 0.4.1, local foundation at 1011623.

## Global Constraints

- User decision 2026-10-23: local dependencies for now; no publish, push, remote creation, submodule or `.gitmodules` changes.
- Work in `/Users/vadim/.codex/worktrees/dependency-alignment/intercom-matter`, branch `codex/dependency-alignment`; preserve both original consumer checkouts and the clean foundation checkout.
- Foundation is `/Users/vadim/personal/projects/embedded-device-foundation`, commit `1011623`; local paths are not revision-pinned by Cargo.lock.
- Track sibling-relative workspace paths `../embedded-device-foundation/crates/...`, not machine-specific absolute paths. The isolated-worktree parent needs a local untracked symlink matching its existing Matter symlinks.
- No dependency version upgrades. Preserve aligned root patches and consumer feature flags.
- Device functionality remains product-owned; shared code must not acquire Intercom/HVAC imports or branches.
- Rust unit tests stay in inline `#[cfg(test)] mod tests`; executable CLI and composed multi-component workflows may live under `tests/`.
- Preserve `IC` serial prefix, `intercom-bridge` hostname prefix, VID `0xfff1`, PID `0x8001`, nine-field pairing JSON, positional MAC, flags, help, serial discovery/fallback and human-readable QR presentation.
- Preserve reset scopes, 2000ms reset reboot delay, watch receiver capacity 3, lifecycle variants and existing public module paths. Ring hold remains 15000ms; auto-relock remains 4 seconds.
- Keep diagnostics and OTA ports local in this stage. In particular, do not introduce the foundation `RebootRequired` OTA error before migrating the OTA driver and HTTP handler together.
- Never access physical serial devices, flash firmware, upload OTA, read private keys, or claim hardware validation.
- Use `CARGO_INCREMENTAL=0 mise exec -- cargo ... --target x86_64-apple-darwin --locked --offline` for host verification. Lock updates use `--offline` without `--locked` once, followed by locked checks.
- One implementation writer at a time, scoped review after each task, final combined review before handoff.
- Preflight finding during Task 1: workspace-wide `cargo fmt --all -- --check` reports existing differences in unrelated consumer and external Matter files. Record that failed gate; require scoped formatting of changed Rust files and do not reformat unrelated repositories.

## Scope and follow-on order

This is the first executable subplan of the approved integration roadmap. The previous 2026-10-19 extraction plan is historical, not execution instructions against the completed APIs. After this stage: GPIO product wrapper; OTA driver plus OTA port and HTTP error mapping; Matter identity/readiness/storage/connection wrappers; diagnostics plus HTTP payload integration; dashboard composition; host script wrappers. Audit and plan each slice against actual interfaces before editing it. Finish Intercom before applying the validated approach to HVAC.

### Task 1: Consume shared pairing generation behind the existing Intercom CLI

**Files:**
- Modify: `Cargo.toml`, `Cargo.lock` (workspace dependency and resolved local package graph).
- Modify: `crates/intercom-pairing-tool/Cargo.toml` (foundation dependency; `intercom-core` no longer needed after tests migrate).
- Modify: `crates/intercom-pairing-tool/src/main.rs` (shared generation, unchanged consumer-owned CLI/rendering, inline unit tests).
- Modify: `crates/intercom-pairing-tool/tests/pairing_tool_tests.rs` (actual executable workflow tests, replacing unit-only tests moved inline).
- Create: `docs/local-foundation-integration.md` (local layout, tested foundation revision, limitations and gates).

**Interfaces:**
- Consumes `edf_pairing_tool::generate(PairingOptions<'_>) -> Result<PairingData, PairingError>`, `PairingData::json(&self) -> String`.
- Produces private `generate_pairing_data(mac: [u8; 6]) -> Result<edf_pairing_tool::PairingData, edf_pairing_tool::PairingError>` used by the existing binary. Consumer parsing remains local, avoiding accidental tightening of its historical accepted MAC syntax.

- [ ] **Step 1: Verify baseline and local layout.** Run `CARGO_INCREMENTAL=0 mise exec -- cargo test -p intercom-pairing-tool --target x86_64-apple-darwin --locked --offline`; expect existing two tests pass. Verify foundation `git status --short` empty and revision 1011623. If absent, create exactly `/Users/vadim/.codex/worktrees/dependency-alignment/embedded-device-foundation` as a symlink to `/Users/vadim/personal/projects/embedded-device-foundation`; never overwrite an existing entry.

- [ ] **Step 2: Wire dependency configuration only.** Add under root workspace dependencies and consumer dependencies respectively:

```toml
# root Cargo.toml [workspace.dependencies]
edf-pairing-tool = { path = "../embedded-device-foundation/crates/edf-pairing-tool" }
# crates/intercom-pairing-tool/Cargo.toml [dependencies]
edf-pairing-tool = { workspace = true }
```

Update the lock with `CARGO_INCREMENTAL=0 mise exec -- cargo check -p intercom-pairing-tool --target x86_64-apple-darwin --offline`. Inspect lock diff: only necessary foundation/transitive package additions, no unrelated updates.

- [ ] **Step 3: Write the failing boundary test first** in main.rs bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_generation_preserves_intercom_reference_values() {
        // Arrange
        let mac = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];

        // Act
        let data = generate_pairing_data(mac).unwrap();

        // Assert
        assert_eq!(
            (data.serial_number.as_str(), data.passcode, data.discriminator,
             data.manual_code.as_str(), data.manual_code_long.as_str(), data.qr_code.as_str()),
            ("IC-98A316A67844", 21710079, 3096, "3012-791-3250",
             "7012-791-3256-55213-27699",
             "MT:-24J042C00X1VC4QK3P0OUYH0R92G1V.X12LJRQ19UZR1OADO1O0")
        );
    }
}
```

Run `CARGO_INCREMENTAL=0 mise exec -- cargo test -p intercom-pairing-tool --bin intercom-pairing-tool --target x86_64-apple-darwin --locked --offline`; expect missing `generate_pairing_data` until implementation. Record actual output, not an invented failure. Add executable characterization before changing existing generation: flags `--mac`, `-m`, positional MAC, `--json`, help and invalid short MAC. Use `env!("CARGO_BIN_EXE_intercom-pairing-tool")` with `std::process::Command`, always explicit MAC (except help) so no hardware autodetection runs. JSON test checks exact expected stdout including all nine fields and final newline; non-JSON test checks existing heading, serial, both codes and QR payload.

- [ ] **Step 4: Implement the shared boundary** above private helpers:

```rust
fn generate_pairing_data(
    mac: [u8; 6],
) -> Result<edf_pairing_tool::PairingData, edf_pairing_tool::PairingError> {
    edf_pairing_tool::generate(edf_pairing_tool::PairingOptions {
        mac,
        prefix: "IC",
        name: "intercom-bridge",
        vid: VID,
        pid: PID,
    })
}
```

Replace the block from `let formatted_mac = format!` through QR payload construction with:

```rust
let data = match generate_pairing_data(mac_bytes) {
    Ok(data) => data,
    Err(error) => {
        eprintln!("Error generating commissioning data: {error:?}");
        std::process::exit(1);
    }
};
let formatted_mac = data.mac.as_str();
let serial_no = data.serial_number.as_str();
let passcode = data.passcode;
let discriminator = data.discriminator;
let short_code = data.manual_code.as_str();
let long_code = data.manual_code_long.as_str();
let qr_text = data.qr_code.as_str();
```

Replace the existing JSON-printing block with:

```rust
if json_mode {
    println!("{}", data.json());
    return;
}
```

Keep local usage/argument parsing, permissive parse_mac, serial querying and terminal output intact. Narrow rs-matter production import to `Qr, QrTextType`; keep rs-matter for matrix rendering. Move existing pure QR round-trip/manual Verhoeff tests inline and exercise generated data/parsed payload instead of rebuilding identity from intercom-core. Then remove `intercom-core` dependency from this host tool, not from firmware or workspace members. Add serde_json workspace dev dependency if using structured executable JSON assertions; avoid new dependency versions.

- [ ] **Step 5: Verify and document.** Run focused tool tests, `cargo fmt --all -- --check`, pairing-tool strict Clippy, then supported consumer host packages (core/http/ota/pairing) in one command with explicit target, locked offline. Document sibling checkout prerequisite, local symlink for isolated worktree, exact foundation revision, unpinned local-path caveat and no submodule/publish/hardware claims. No release firmware rebuild is needed for this host-only slice; device dependencies are unchanged.

- [ ] **Step 6: Commit and review.** Commit only named task files with message `refactor: reuse local foundation pairing generation`. Report baseline, real RED/GREEN output, exact command/test totals, lock diff, changed files and concerns to `.superpowers/sdd/local-integration-task-1-report.md`. Controller generates task review package from recorded pre-dispatch base and requires spec + quality approval.

### Task 2: Share lifecycle, ecosystem and reset contracts through compatibility paths

**Files:**
- Modify: root `Cargo.toml`, `Cargo.lock`, `crates/intercom-core/Cargo.toml`.
- Modify: `crates/intercom-core/src/events.rs`, `domain/ecosystem.rs`, `domain/reset.rs`, `ports/ecosystem.rs`, `ports/reset.rs`, `ports/mod.rs`, `service/reset.rs`, `config.rs`.
- Modify: `docs/local-foundation-integration.md` with evidence and completed coverage.

**Interfaces:**
- Consumes public `edf_core::{events, domain::{ecosystem,reset}, ports::{ecosystem,reset}, service::ResetService}`.
- Produces the existing `intercom_core` module paths as re-exports of the same types and traits, so downstream drivers implement the foundation contracts without orphan implementations for foreign types.
- Keeps local diagnostics types/service/hardware port and OTA port unchanged; keeps access, ring, command handles and commissioning policy local.

- [ ] **Step 1: Add dependency configuration and failing interoperability tests.** Root workspace dependency `edf-core = { path = "../embedded-device-foundation/crates/edf-core" }`; intercom-core dependency `edf-core = { workspace = true }`. Resolve lock offline. Append below existing events code:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foundation_sender_updates_consumer_watch() {
        // Arrange
        let watch = SystemStateWatch::new();

        // Act
        edf_core::SystemStateSender::new(&watch).operational();

        // Assert
        assert_eq!(watch.try_get(), Some(SystemState::Operational));
    }
}
```

Add this inline test to the existing bottom test module of domain/reset.rs:

```rust
#[test]
fn consumer_scope_is_the_foundation_scope() {
    // Arrange
    let scope = ResetScope::Factory;

    // Act
    let shared: edf_core::ResetScope = scope;

    // Assert
    assert_eq!(shared, edf_core::ResetScope::Factory);
}
```

Append this inline module to domain/ecosystem.rs:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumer_status_is_the_foundation_status() {
        // Arrange
        let status = EcosystemStatus::new(EcosystemId::new("sample"), true);

        // Act
        let shared: edf_core::EcosystemStatus = status;

        // Assert
        assert_eq!((shared.ecosystem_type.as_str(), shared.paired), ("sample", true));
    }
}
```

Run core `--lib` locked offline; expect distinct local/foundation-type mismatches before replacing implementations.

- [ ] **Step 2: Replace duplicated implementation with explicit re-exports.** Each source file retains its module doc and inline consumer interoperability/characterization tests, replacing its implementation with exactly the corresponding export:

```rust
// src/events.rs
pub use edf_core::events::{
    SystemState, SystemStateReceiver, SystemStateSender, SystemStateWatch, STATE_WATCH_RECEIVERS,
};
// src/domain/ecosystem.rs
pub use edf_core::domain::ecosystem::{
    EcosystemId, EcosystemStatus, EcosystemType, EcosystemValue, EndpointInfo,
    FabricInfo, FabricLabel, MAX_ECOSYSTEM_ENDPOINTS, MAX_ECOSYSTEM_FABRICS,
    MAX_ECOSYSTEM_PROPERTIES,
};
// src/domain/reset.rs
pub use edf_core::domain::reset::{classify_reset_hold, ResetScope};
// src/ports/ecosystem.rs
pub use edf_core::ports::ecosystem::EcosystemInfoDriver;
// src/ports/reset.rs
pub use edf_core::ports::reset::{ResetScope, ResetTriggerDriver, SystemResetDriver};
// src/service/reset.rs
pub use edf_core::service::ResetService;
```

In ports/mod.rs replace reset re-export with `pub use reset::{ResetScope, ResetTriggerDriver, SystemResetDriver};` and remove only the local ResetTriggerDriver trait block. In config.rs replace the duplicated reboot-delay declaration with `pub use edf_core::config::FACTORY_RESET_REBOOT_DELAY_MS;` above private items, retaining both access policy constants. Remove superseded implementation imports. Keep all six reset-hold characterization tests inline; shared unit implementation coverage also remains in foundation. Do not move consumer tests to a separate unit-test file.

- [ ] **Step 3: Verify composition and MCU build.** Run core focused tests; then supported core/http/ota/pairing host tests, firmware `--lib`, firmware `--test partitioned_kv_tests`, all explicit macOS target, locked offline. Run fmt check; strict Clippy for core. Run `CARGO_INCREMENTAL=0 mise exec -- cargo build -p intercom-firmware --release --locked --offline`; expect riscv32imac release success. Do not add GPIO/Matter to macOS host tests: they are MCU-only. Check release image fits 0x350000-byte OTA slot (3473408 bytes), using the same ESP image conversion as phase 0 without hardware access. If trait coherence/build failures appear, report exact evidence and resolve consumer-owned wrappers; never reintroduce duplicate shared types.

- [ ] **Step 4: Commit and review.** Update integration document with commands/results, remaining migration scope, and no hardware claim. Commit named task files as `refactor: share foundation lifecycle and reset contracts`. Write report `.superpowers/sdd/local-integration-task-2-report.md`, including RED/GREEN and MCU evidence. Require scoped review, then controller fresh validation and combined final review of both tasks.

## Self-review

The stage follows approved dependency direction and user-approved local distribution. Public signatures above were checked against foundation 1011623. Existing consumer parsing/UX stays local. Generic diagnostics and OTA error additions are explicitly excluded to avoid an unsafe partial migration; follow-on slices remain required for full integration. The dependency configuration is intentionally not a portable published distribution or immutable pin.
