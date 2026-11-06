# Spec Delta: Downstream Intercom Bridge Integration & Verification

## Purpose

Integrates upstream releases of `project-chip/rs-matter` and `sysgrok/rs-matter-embassy` into the `intercom-matter` workspace, removing temporary local path patches and verifying firmware stability.

## ADDED Requirements

### Requirement: Upstream Dependency Alignment
The workspace `Cargo.toml` SHALL consume upstream release tags or branches of `rs-matter` and `rs-matter-embassy` with zero local path patches in `[patch.crates-io]`.

#### Scenario: Building without local repository patches
- **WHEN** the patch section is removed from `Cargo.toml`
- **THEN** Cargo resolves all dependencies cleanly from upstream git/crates.io

### Requirement: Host Test Suite Verification
All 107 host unit and integration tests across `intercom-core`, `intercom-adapter-http`, and `intercom-pairing-tool` SHALL pass cleanly without regressions.

#### Scenario: Running host test suite
- **WHEN** `cargo test --target x86_64-apple-darwin` is executed
- **THEN** all 107 tests report ok with 0 failures

### Requirement: ESP32-C6 DevKit Firmware Budget
The release DevKit firmware binary SHALL build with `cargo build-devkit` and link cleanly within the 48 KiB execution stack allocation.

#### Scenario: DevKit release build
- **WHEN** `cargo build-devkit` compiles `intercom-firmware`
- **THEN** the compiler and linker produce a valid ELF image within SRAM partition limits
