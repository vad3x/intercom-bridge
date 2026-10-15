# Remote Firmware Updates (OTA) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable secure, wireless Over-The-Air (OTA) firmware updates for the Smart Intercom Bridge over local Wi-Fi with zero cables, automatic mDNS multi-device discovery, Ed25519 cryptographic signing, and automatic ESP-IDF bootloader rollback.

**Architecture:** Ports & Adapters (Hexagonal Architecture). An outbound adapter `intercom-adapter-ota` runs an unbuffered, zero-heap async HTTP listener on port 8080 over Embassy networking. It streams incoming signed firmware chunks into the inactive flash slot (`ota_0` or `ota_1`), validates the Ed25519 signature before updating `otadata`, and leverages the 2nd-stage ESP-IDF bootloader with automatic rollback to ensure zero bricking risk.

**Tech Stack:** Rust (`no_std`, `riscv32imac-unknown-none-elf`), `esp-hal`, `esp-rtos`, `embassy-net`, `embassy-sync`, `ed25519-dalek`, Python 3 (`zeroconf`, `cryptography`, `urllib`).

## Global Constraints
- Target hardware: ESP32-C6-WROOM-1-N8 (8 MB Quad SPI Flash).
- Flash layout: `nvs` at 0x9000 (24 KB), `otadata` at 0xf000 (8 KB), `ota_0` at 0x20000 (3.4 MB), `ota_1` at 0x370000 (3.4 MB), `matter_kvs` at 0x6c0000 (24 KB).
- Zero heap allocation in network streaming pipeline (fixed static 4 KB sector buffer).
- Single-connection TCP concurrency limit on OTA port 8080.
- Asymmetric Ed25519 signature verification: private key never touches the device, 32-byte public key embedded in firmware.
- Priority preemption: Door strike and Ring optocoupler interrupts always preempt OTA streaming.
- Standard Arrange-Act-Assert structure in all unit tests.
- Public items ordered above private items in all Rust files.

---

### Task 1: Flash Partition Table & Memory Configuration

**Files:**
- Modify: `partitions.csv`
- Modify: `crates/intercom-firmware/src/config.rs:28-40`

**Interfaces:**
- Consumes: ESP-IDF partition specification.
- Produces: `MATTER_KVS_OFFSET = 0x6c0000`, `OTA_0_OFFSET = 0x20000`, `OTA_1_OFFSET = 0x370000`, `OTA_SLOT_SIZE = 0x350000`.

- [x] **Step 1: Update `partitions.csv` with 8MB A/B Dual-Bank Scheme**

Edit `partitions.csv`:
```csv
# Name,       Type, SubType, Offset,   Size,     Flags
nvs,          data, nvs,     0x9000,   0x6000,
otadata,      data, ota,     0xf000,   0x2000,
ota_0,        app,  ota_0,   0x20000,  0x350000,
ota_1,        app,  ota_1,   0x370000, 0x350000,
matter_kvs,   data, nvs,     0x6c0000, 0x6000,
```

- [x] **Step 2: Update `crates/intercom-firmware/src/config.rs` Offsets**

Update `crates/intercom-firmware/src/config.rs`:
```rust
// ============================================================================
// Flash persistence & Partition Layout (from partitions.csv)
// ============================================================================

/// Start offset of the `otadata` partition in flash.
pub const OTA_DATA_OFFSET: u32 = 0x00f000;
/// Size of the `otadata` partition (8 KiB = 2 sectors).
pub const OTA_DATA_SIZE: u32 = 0x002000;

/// Start offset of `ota_0` app partition (Slot A).
pub const OTA_0_OFFSET: u32 = 0x020000;
/// Start offset of `ota_1` app partition (Slot B).
pub const OTA_1_OFFSET: u32 = 0x370000;
/// Size of each app partition (3.4 MiB = 896 sectors × 4 KiB).
pub const OTA_SLOT_SIZE: u32 = 0x350000;

/// Start offset of the `matter_kvs` partition in flash (from partitions.csv).
pub const MATTER_KVS_OFFSET: u32 = 0x6c0000;

/// Size of the `matter_kvs` partition (24 KiB = 6 sectors × 4 KiB).
pub const MATTER_KVS_SIZE: u32 = 0x6000;

/// Flash range for the Matter KV blob store.
pub const MATTER_KVS_RANGE: Range<u32> =
    MATTER_KVS_OFFSET..MATTER_KVS_OFFSET + MATTER_KVS_SIZE;
```

- [x] **Step 3: Verify Firmware Compiles with New Layout**

Run: `mise exec -- cargo build -p intercom-firmware`
Expected: Successful compile with zero errors.

- [x] **Step 4: Commit Partition Configuration**

```bash
git add partitions.csv crates/intercom-firmware/src/config.rs
git commit -m "feat(flash): configure 8MB dual-bank A/B partition layout for OTA"
```

---

### Task 2: Core Lifecycle State Extension & LED Indicator Strobe

**Files:**
- Modify: `crates/intercom-core/src/events.rs:9-26`
- Modify: `crates/intercom-adapter-gpio/src/led.rs`
- Modify: `crates/intercom-adapter-gpio/src/config.rs`

**Interfaces:**
- Consumes: `SystemStateWatch`, `SystemStateReceiver`.
- Produces: `SystemState::Updating`, `IndicatorCommand::Strobe`.

- [x] **Step 1: Write Unit Test for `SystemState::Updating` in `intercom-core`**

Add test to `crates/intercom-core/tests/` (or in `events.rs` test module):
```rust
#[test]
fn system_state_watch_notifies_updating_transition() {
    // Arrange
    let watch = SystemStateWatch::new();
    let sender = SystemStateSender::new(&watch);
    let mut receiver = watch.receiver().unwrap();

    // Act
    sender.send(SystemState::Updating);

    // Assert
    assert_eq!(receiver.try_get(), Ok(SystemState::Updating));
}
```

- [x] **Step 2: Add `SystemState::Updating` Variant**

In `crates/intercom-core/src/events.rs`:
```rust
pub enum SystemState {
    Initializing,
    Initialized,
    Connecting,
    Connected,
    Ready,
    /// Firmware OTA update in progress (flashing flash memory)
    Updating,
    FactoryReset,
    Error,
}
```

- [x] **Step 3: Add Strobe Pattern to `GpioLedDriver`**

In `crates/intercom-adapter-gpio/src/config.rs`:
```rust
/// LED rapid strobe period during OTA update (50ms ON, 50ms OFF = 10 Hz)
pub const UPDATING_STROBE_MS: u64 = 50;
```

In `crates/intercom-adapter-gpio/src/led.rs`:
Add `IndicatorCommand::Strobe` to command enum and map `SystemState::Updating` to trigger `IndicatorCommand::Strobe` in `observe_state`.

- [x] **Step 4: Run Tests on Core and GPIO Adapter**

Run: `cargo test -p intercom-core --target x86_64-apple-darwin`
Expected: PASS.

- [x] **Step 5: Commit Lifecycle & Indicator Updates**

```bash
git add crates/intercom-core/ crates/intercom-adapter-gpio/
git commit -m "feat(core): add SystemState::Updating and rapid LED strobe pattern"
```

---

### Task 3: Host Key Generation & OTA Push CLI Scripts

**Files:**
- Create: `scripts/generate_keys.py`
- Create: `scripts/ota.py`

**Interfaces:**
- Consumes: Local binary file, Wi-Fi network interface.
- Produces: Signed packet format `[b"ICOT"][u32 length][64-byte Ed25519 sig][binary...]` over HTTP POST to target.

- [x] **Step 1: Create `scripts/generate_keys.py`**

```python
#!/usr/bin/env python3
"""Generate an Ed25519 key pair for Smart Intercom OTA firmware signing."""

import os
from pathlib import Path
from cryptography.hazmat.primitives.asymmetric import ed25519
from cryptography.hazmat.primitives import serialization

KEY_DIR = Path.home() / ".intercom"
KEY_PATH = KEY_DIR / "ota_key.pem"

def main():
    KEY_DIR.mkdir(parents=True, exist_ok=True)
    if KEY_PATH.exists():
        print(f"Key already exists at {KEY_PATH}")
        with open(KEY_PATH, "rb") as f:
            private_key = serialization.load_pem_private_key(f.read(), password=None)
    else:
        private_key = ed25519.Ed25519PrivateKey.generate()
        pem = private_key.private_bytes(
            encoding=serialization.Encoding.PEM,
            format=serialization.PrivateFormat.PKCS8,
            encryption_algorithm=serialization.NoEncryption()
        )
        with open(KEY_PATH, "wb") as f:
            f.write(pem)
        KEY_PATH.chmod(0o600)
        print(f"Generated new Ed25519 private key: {KEY_PATH}")

    public_key = private_key.public_key()
    pub_bytes = public_key.public_bytes(
        encoding=serialization.Encoding.Raw,
        format=serialization.PublicFormat.Raw
    )
    rust_bytes = ", ".join(f"0x{b:02x}" for b in pub_bytes)
    print("\nCopy this public key into your firmware config:")
    print(f"pub const OTA_PUBLIC_KEY: [u8; 32] = [{rust_bytes}];\n")

if __name__ == "__main__":
    main()
```

- [x] **Step 2: Create `scripts/ota.py` Multi-Device Pusher**

Create `scripts/ota.py`:
- Uses `zeroconf` to browse `_intercom-ota._tcp.local.` services.
- If multiple devices found, prompt user (`[1] Front Door`, `[2] Back Gate`, `[A] All`) or accept `--target`.
- Loads `~/.intercom/ota_key.pem`, computes Ed25519 signature over binary.
- Packages header: `b"ICOT"` (4 bytes) + `len` (4 bytes little-endian) + `signature` (64 bytes).
- Sends `POST http://<ip>:8080/api/v1/ota/flash` with streaming body and progress bar.

- [x] **Step 3: Test Key Generation on Host**

Run: `python3 scripts/generate_keys.py`
Expected: Generates key in `~/.intercom/ota_key.pem` and outputs 32-byte public key array.

- [x] **Step 4: Commit Host Tools**

```bash
git add scripts/generate_keys.py scripts/ota.py
git commit -m "feat(tools): add Ed25519 key generator and multi-device OTA push script"
```

---

### Task 4: `intercom-adapter-ota` Crate & Bootloader State Module

**Files:**
- Create: `crates/intercom-adapter-ota/Cargo.toml`
- Create: `crates/intercom-adapter-ota/src/lib.rs`
- Create: `crates/intercom-adapter-ota/src/bootloader.rs`
- Modify: `Cargo.toml` (add to workspace members)

**Interfaces:**
- Consumes: `otadata` flash partition.
- Produces: `OtaBootloader` with `get_running_slot()`, `get_target_slot()`, `mark_pending_verify(target_slot, seq)`, and `mark_valid()`.

- [x] **Step 1: Write Unit Test for Bootloader `esp_ota_select_entry_t` CRC & Sequence Resolution**

In `crates/intercom-adapter-ota/src/bootloader.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_slot_with_higher_valid_sequence() {
        // Arrange
        let mut sector0 = [0xffu8; 4096];
        let mut sector1 = [0xffu8; 4096];
        write_ota_entry(&mut sector0, 1, OtaState::Valid);
        write_ota_entry(&mut sector1, 2, OtaState::Valid);

        // Act
        let active = resolve_active_slot(&sector0, &sector1);

        // Assert
        assert_eq!(active.seq, 2);
        assert_eq!(active.slot, Slot::Ota1);
    }
}
```

- [x] **Step 2: Create `crates/intercom-adapter-ota/Cargo.toml`**

```toml
[package]
name = "intercom-adapter-ota"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"

[dependencies]
intercom-core = { path = "../intercom-core" }
log = "0.4"
embassy-net = { version = "0.9", features = ["tcp"] }
embassy-time = "0.5"
embassy-sync = "0.8"
ed25519-dalek = { version = "2", default-features = false }
sha2 = { version = "0.10", default-features = false }
crc32fast = { version = "1.4", default-features = false }

[target.'cfg(target_arch = "riscv32")'.dependencies]
esp-hal = { version = "~1.2", features = ["esp32c6"] }
```

- [x] **Step 3: Implement `bootloader.rs`**

Implement `esp_ota_select_entry_t` parser, CRC-32 verification, active slot detection, and sequence increment logic.

- [x] **Step 4: Run Host Unit Tests on Bootloader Module**

Run: `cargo test -p intercom-adapter-ota --target x86_64-apple-darwin`
Expected: PASS.

- [x] **Step 5: Commit `intercom-adapter-ota` Scaffolding**

```bash
git add Cargo.toml crates/intercom-adapter-ota/
git commit -m "feat(ota): create intercom-adapter-ota crate with ESP-IDF bootloader state machine"
```

---

### Task 5: Flash Sector Writer & Ed25519 Verifier

**Files:**
- Create: `crates/intercom-adapter-ota/src/writer.rs`
- Modify: `crates/intercom-adapter-ota/src/lib.rs`

**Interfaces:**
- Consumes: Byte stream, `OTA_PUBLIC_KEY: [u8; 32]`, active flash partition boundaries.
- Produces: `OtaWriter` struct with `write_chunk(&mut self, chunk: &[u8]) -> Result<(), OtaError>` and `finish(&mut self) -> Result<(), OtaError>`.

- [x] **Step 1: Write Unit Test for Stream Signature Verification**

In `crates/intercom-adapter-ota/src/writer.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_image_with_tampered_payload() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let payload = vec![0x12; 8192];
        let sig = sign_payload(&privkey, &payload);
        let mut tampered = payload.clone();
        tampered[100] ^= 0x01; // flip 1 bit

        // Act
        let result = verify_image_signature(&pubkey, &tampered, &sig);

        // Assert
        assert!(result.is_err());
    }
}
```

- [x] **Step 2: Implement `OtaWriter`**

Implement `OtaWriter`:
- Maintains rolling SHA-512 digest.
- Validates `[b"ICOT"][u32 size][64-byte sig]` header.
- Buffers bytes into 4 KB sector aligned buffer.
- On RISC-V target, writes via `esp_hal::rom::spiflash::esp_rom_spiflash_erase_sector` and `esp_rom_spiflash_write`.
- Verifies Ed25519 signature on completion before returning success.

- [x] **Step 3: Run Host Unit Tests**

Run: `cargo test -p intercom-adapter-ota --target x86_64-apple-darwin`
Expected: PASS.

- [x] **Step 4: Commit Flash Writer Module**

```bash
git add crates/intercom-adapter-ota/src/writer.rs crates/intercom-adapter-ota/src/lib.rs
git commit -m "feat(ota): implement streaming flash writer with Ed25519 verification"
```

---

### Task 6: Async HTTP OTA Server (`server.rs`)

**Files:**
- Create: `crates/intercom-adapter-ota/src/server.rs`
- Modify: `crates/intercom-adapter-ota/src/lib.rs`

**Interfaces:**
- Consumes: `embassy_net::Stack`, `SystemStateSender`, `OTA_PUBLIC_KEY`.
- Produces: `OtaServer::run(&self) -> !`.

- [x] **Step 1: Write Unit Test for HTTP Request Header Parsing**

In `crates/intercom-adapter-ota/src/server.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_ota_post_request() {
        // Arrange
        let request = b"POST /api/v1/ota/flash HTTP/1.1\r\nContent-Length: 1048576\r\n\r\n";

        // Act
        let parsed = parse_request_headers(request);

        // Assert
        assert_eq!(parsed, Ok(RequestKind::Flash(1048576)));
    }
}
```

- [x] **Step 2: Implement `OtaServer`**

Implement `OtaServer`:
- Binds TCP port 8080.
- Limits concurrency: 1 socket, immediate drop if busy.
- Streams payload into `OtaWriter`.
- Signals `SystemState::Updating` to LED indicator.
- Upon successful signature verification: updates `otadata` to `ESP_OTA_IMG_NEW`, sends `HTTP/1.1 200 OK`, yields 500ms, and calls `esp_hal::system::software_reset()`.
- Upon failure: sends `HTTP/1.1 400 Bad Request` and imposes 5-second tarpit delay.

- [x] **Step 3: Run Host Unit Tests on Server Module**

Run: `cargo test -p intercom-adapter-ota --target x86_64-apple-darwin`
Expected: PASS.

- [x] **Step 4: Commit OTA Server Module**

```bash
git add crates/intercom-adapter-ota/src/server.rs crates/intercom-adapter-ota/src/lib.rs
git commit -m "feat(ota): implement async HTTP OTA server with connection throttling"
```

---

### Task 7: Composition Root Integration & Startup Health Confirmation

**Files:**
- Modify: `crates/intercom-firmware/Cargo.toml`
- Modify: `crates/intercom-firmware/src/config.rs`
- Modify: `crates/intercom-firmware/src/main.rs`

**Interfaces:**
- Consumes: `intercom-adapter-ota::OtaServer`, `intercom-adapter-ota::bootloader::mark_valid`.
- Produces: Concurrent OTA task in `main.rs`.

- [x] **Step 1: Add `intercom-adapter-ota` to Firmware Dependencies**

In `crates/intercom-firmware/Cargo.toml`:
```toml
intercom-adapter-ota = { path = "../intercom-adapter-ota" }
```

- [x] **Step 2: Add Embedded Public Key in `crates/intercom-firmware/src/config.rs`**

```rust
/// Ed25519 public key used to verify incoming OTA firmware images.
pub const OTA_PUBLIC_KEY: [u8; 32] = [
    // Output from scripts/generate_keys.py
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
```

- [x] **Step 3: Wire Startup Validation & OTA Task in `crates/intercom-firmware/src/main.rs`**

In `crates/intercom-firmware/src/main.rs`:
- On boot: call `intercom_adapter_ota::bootloader::mark_running_slot_valid()` to cancel any pending rollback.
- Instantiate `OtaServer::new(stack, state_sender, config::OTA_PUBLIC_KEY)`.
- Add `ota_server.run()` to `embassy_futures::join::join4(app_future, matter_future, led_future, ota_future)`.

- [x] **Step 4: Build Firmware Binary for RISC-V Target**

Run: `mise exec -- cargo build -p intercom-firmware --release`
Expected: Successful compile and link.

- [x] **Step 5: Commit Firmware Composition Root Wiring**

```bash
git add crates/intercom-firmware/
git commit -m "feat(firmware): wire OTA server into composition root and confirm startup slot"
```

---

### Task 8: End-to-End Verification & Documentation

**Files:**
- Modify: `AGENTS.md` (add OTA documentation and commands)
- Create: `docs/superpowers/plans/2026-10-15-remote-firmware-ota.md`

- [x] **Step 1: Update `AGENTS.md` with OTA Instructions**

Document:
- Generating OTA keys (`python3 scripts/generate_keys.py`).
- Flashing firmware remotely via Wi-Fi (`python3 scripts/ota.py target/riscv32imac-unknown-none-elf/release/intercom-bridge`).
- Multi-device discovery and `--target` options.
- Rollback diagnostics and partition layout.

- [x] **Step 2: Run Full Host Test Suite**

Run: `cargo test -p intercom-core -p intercom-adapter-ota --target x86_64-apple-darwin`
Expected: All tests pass (0 failures).

- [x] **Step 3: Final Git Commit**

```bash
git add AGENTS.md
git commit -m "docs: add remote firmware updates (OTA) operational guidelines"
```
