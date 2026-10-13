# Modernize Matter and ESP Ecosystem Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bump the entire stack (`rs-matter` to v0.4.1+, `rs-matter-stack` to v0.3.1, `rs-matter-embassy` to latest upstream with trouble, and `esp-hal` to v1.2 / crates.io releases) while preserving all fork features (`last_case_activity`, MLDv1 rewriting, neighbor injection, spare UDP sockets).

**Architecture:**
- Fork branches are updated on upstream heads with cherry-picked custom features.
- `intercom-matter` moves to a new branch (`rust-4`) targeting the modern crates.
- Replace git rev dependencies of `esp-hal`, `esp-radio`, `esp-rtos`, etc., with clean crates.io releases (`esp-hal ~1.2`, `esp-radio 1.0.0-beta.1`, `esp-rtos 0.4.0`, etc.).
- Update `src/main.rs` interrupt controller token to `peripherals.FROM_CPU_INTR0`.

**Tech Stack:**
- Rust nightly targeting `riscv32imac-unknown-none-elf`
- `esp-hal 1.2.2`, `esp-radio 1.0.0-beta.1`, `esp-rtos 0.4.0`, `esp-alloc 0.11.0`, `esp-backtrace 0.20.0`, `esp-println 0.18.0`
- `rs-matter 0.4.1`, `rs-matter-stack 0.3.1`, `rs-matter-embassy 0.1.0` (with `trouble 0.8.0`)

## Global Constraints
- Target platform: ESP32-C6 (`riscv32imac-unknown-none-elf`, `no_std`).
- All tests/checks MUST run through `cargo check` / `cargo build`.
- No regression of custom features: `last_case_activity()`, MLDv1 IPv6 rewriting, spare UDP sockets, synthetic NS injection.
- Keep `intercom-matter` architecture clean: public before private, drivers decoupled with sync callbacks.

---

### Task 1: Rebase `rs-matter` Fork to Upstream `main` (`0.4.1`)

**Files:**
- Repository: `../rs-matter`
- Modify: `rs-matter/src/lib.rs`, `rs-matter/src/transport/session.rs`

**Interfaces:**
- Consumes: `upstream/main`
- Produces: `Matter::last_case_activity(&self) -> Option<embassy_time::Instant>`

- [ ] **Step 1: Create modern branch on `rs-matter`**
  Create branch `compat-0.4` from `upstream/main`.
- [ ] **Step 2: Cherry-pick / Port `last_case_activity()`**
  Apply the `last_case_activity` logic to `rs-matter/src/lib.rs` and `rs-matter/src/transport/session.rs`.
- [ ] **Step 3: Verify `rs-matter` builds for RISC-V**
  Run `cargo check --manifest-path ../rs-matter/rs-matter/Cargo.toml --target riscv32imac-unknown-none-elf --no-default-features --features rustcrypto`.

---

### Task 2: Align `rs-matter-stack` Fork (`0.3.1`)

**Files:**
- Repository: `../rs-matter-stack`
- Target Branch: `compat-0.3` from `upstream/master` (tag `v0.3.1`)

**Interfaces:**
- Consumes: `rs-matter 0.4.1`
- Produces: `rs-matter-stack 0.3.1` compatible with modern `rs-matter`

- [ ] **Step 1: Create branch `compat-0.3` on `rs-matter-stack`**
  Checkout `upstream/master` into `compat-0.3`.
- [ ] **Step 2: Point `rs-matter` dependency to local path**
  Ensure path override or patch to local `rs-matter` works if needed.
- [ ] **Step 3: Verify `rs-matter-stack` builds for RISC-V**
  Run `cargo check --manifest-path ../rs-matter-stack/Cargo.toml --target riscv32imac-unknown-none-elf --no-default-features --features rustcrypto`.

---

### Task 3: Port Network Fixes into `rs-matter-embassy` on `upstream/master`

**Files:**
- Repository: `../rs-matter-embassy`
- Target Branch: `compat-0.1` from `upstream/master`
- Modify: `rs-matter-embassy/src/enet.rs`, `rs-matter-embassy/Cargo.toml`

**Interfaces:**
- Consumes: `rs-matter-stack 0.3.1`, `esp-hal 1.2`, `esp-radio 1.0.0-beta.1`
- Produces: `rs-matter-embassy` with MLDv1 translation, neighbor learning, and spare UDP sockets.

- [ ] **Step 1: Create branch `compat-0.1` on `rs-matter-embassy`**
  Checkout `upstream/master` into `compat-0.1`.
- [ ] **Step 2: Port network stack enhancements into `enet.rs`**
  Port MLDv1 report translation, synthetic NS cache injection, and spare UDP socket reservation from `fix/ipv6-multicast-mld-snooping-compat`.
- [ ] **Step 3: Verify `rs-matter-embassy` compiles for ESP32-C6**
  Run `cargo check --manifest-path ../rs-matter-embassy/rs-matter-embassy/Cargo.toml --target riscv32imac-unknown-none-elf --no-default-features --features "esp,esp-hal/esp32c6,esp-radio/esp32c6,esp-radio/unstable,trouble,embassy-net,log,rustcrypto"`.

---

### Task 4: Modernize `intercom-matter` Dependencies and Code

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/main.rs`
- Modify: `src/drivers/matter/controller.rs`

**Interfaces:**
- Consumes: updated local forks (`rs-matter`, `rs-matter-stack`, `rs-matter-embassy`), crates.io `esp-hal ~1.2`
- Produces: Working RISC-V binary on branch `rust-4`.

- [ ] **Step 1: Create `rust-4` branch in `intercom-matter`**
  Create `rust-4` from `rust-3`.
- [ ] **Step 2: Update `Cargo.toml`**
  Update dependency versions to crates.io releases:
  - `esp-hal = { version = "~1.2", features = ["log-04", "unstable", "exception-handler"] }`
  - `esp-radio = { version = "1.0.0-beta.1", features = ["ble", "log-04", "unstable"] }`
  - `esp-rtos = { version = "0.4", features = ["esp-radio", "embassy"] }`
  - `esp-backtrace = { version = "0.20", features = ["panic-handler", "println", "custom-halt"] }`
  - `esp-println = { version = "0.18", features = ["log-04"] }`
  - `esp-alloc = { version = "0.11" }`
  - `esp-bootloader-esp-idf = { version = "0.6", features = ["log-04"] }`
  - `esp-metadata-generated = "0.5"`
  - `rs-matter-embassy` with feature `trouble`
  - Remove all `git = "https://github.com/esp-rs/esp-hal"` patches in `[patch.crates-io]`!
- [ ] **Step 3: Update `src/main.rs` hardware initialization**
  Change interrupt controller initialization to `peripherals.FROM_CPU_INTR0`.
- [ ] **Step 4: Resolve build errors and warnings**
  Fix any type or API discrepancies in `controller.rs` or `events.rs`.
- [ ] **Step 5: Run full verification build**
  Execute `cargo build` and verify output binary.
