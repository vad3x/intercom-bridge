# Matter Commissioning QR & Pairing Code Tool Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide a `cargo qr` command and `scripts/qr.py` helper script that reads the MAC address of a connected ESP32-C6 and displays the Matter commissioning QR code and manual pairing codes in the terminal.

**Architecture:** Pure MAC derivation in `intercom-core`, host CLI `intercom-pairing-tool` using `rs-matter` directly for QR matrix and manual code generation, and a Python launcher `scripts/qr.py` detecting USB devices via `espflash board-info` and invoking the host binary.

**Tech Stack:** Rust (`no_std` `intercom-core`, host `rs-matter`), Python 3 (`espflash`), Cargo alias.

## Global Constraints

- Preserve all existing tests and docstrings across `intercom-core` and `intercom-adapter-matter`.
- All tests must follow Arrange → Act → Assert pattern with explicit comments.
- Public items before private items within every file.
- High-level abstractions above low-level details.
- No new external pip package requirements for the Python script.

---

### Task 1: Extract Pure MAC Derivation Logic to `intercom-core`

**Files:**
- Create: `crates/intercom-core/src/domain/commissioning.rs`
- Modify: `crates/intercom-core/src/domain/mod.rs`
- Modify: `crates/intercom-adapter-matter/src/commissioning.rs`
- Test: `crates/intercom-core/src/domain/commissioning.rs` (unit tests)

**Interfaces:**
- Produces:
  ```rust
  pub const FORBIDDEN_PASSCODES: [u32; 10];
  pub const MIN_PASSCODE: u32 = 10_000_001;
  pub const MAX_PASSCODE: u32 = 99_999_998;
  pub const MIN_DISCRIMINATOR: u16 = 1;
  pub const MAX_DISCRIMINATOR: u16 = 4095;
  pub const TEST_DISCRIMINATOR: u16 = 3840;

  pub fn mac_hash(mac: &[u8; 6]) -> u64;
  pub fn passcode_from_mac(mac: &[u8; 6]) -> u32;
  pub fn discriminator_from_mac(mac: &[u8; 6]) -> u16;
  pub fn is_forbidden_passcode(passcode: u32) -> bool;
  pub const fn serial_no_from_mac(mac: &[u8; 6]) -> [u8; 15];
  pub const fn hostname_from_mac(mac: &[u8; 6]) -> [u8; 20];
  ```

- [ ] **Step 1: Create `crates/intercom-core/src/domain/commissioning.rs` with domain math and tests**

```rust
//! Matter commissioning parameters generation.
//!
//! Derives unique, deterministic Matter commissioning parameters (Passcode and
//! Discriminator) from the hardware MAC address instead of using hardcoded test values.

// ============================================================================
// Public Constants
// ============================================================================

/// Forbidden passcodes per Matter specification (Section 5.1.7.1).
pub const FORBIDDEN_PASSCODES: [u32; 10] = [
    11111111, 22222222, 33333333, 44444444,
    55555555, 66666666, 77777777, 88888888,
    12345678, 87654321,
];

/// Minimum 8-digit passcode for generated codes (avoids small/test numbers).
pub const MIN_PASSCODE: u32 = 10_000_001;

/// Maximum allowed passcode per Matter specification (Section 5.1.7.1).
pub const MAX_PASSCODE: u32 = 99_999_998;

/// Minimum allowed discriminator (avoids 0 which may be treated as unset).
pub const MIN_DISCRIMINATOR: u16 = 1;

/// Maximum allowed 12-bit discriminator (0x0FFF).
pub const MAX_DISCRIMINATOR: u16 = 4095;

/// Well-known test discriminator used by rs-matter and chip-tool examples.
pub const TEST_DISCRIMINATOR: u16 = 3840;

/// Standard length of formatted serial number `"IC-XXXXXXXXXXXX"`.
pub const MAC_SERIAL_NO_LEN: usize = 15;

/// Standard length of formatted DHCP hostname `"intercom-bridge-xxxx"`.
pub const MAC_HOSTNAME_LEN: usize = 20;

// ============================================================================
// Public Functions
// ============================================================================

/// Derives a standard serial number buffer `"IC-XXXXXXXXXXXX"` from a 6-byte MAC address.
pub const fn serial_no_from_mac(mac: &[u8; 6]) -> [u8; MAC_SERIAL_NO_LEN] {
    const HEX_CHARS: &[u8; 16] = b"0123456789ABCDEF";
    let mut buf = [0u8; MAC_SERIAL_NO_LEN];
    buf[0] = b'I';
    buf[1] = b'C';
    buf[2] = b'-';

    let mut i = 0;
    while i < 6 {
        let byte = mac[i];
        buf[3 + i * 2] = HEX_CHARS[(byte >> 4) as usize];
        buf[3 + i * 2 + 1] = HEX_CHARS[(byte & 0x0F) as usize];
        i += 1;
    }
    buf
}

/// Derives a standard DHCP hostname buffer `"intercom-bridge-xxxx"` from a 6-byte MAC address.
pub const fn hostname_from_mac(mac: &[u8; 6]) -> [u8; MAC_HOSTNAME_LEN] {
    const PREFIX: &[u8] = b"intercom-bridge-";
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";
    let mut buf = [0u8; MAC_HOSTNAME_LEN];

    let mut i = 0;
    while i < PREFIX.len() {
        buf[i] = PREFIX[i];
        i += 1;
    }

    buf[16] = HEX_CHARS[(mac[4] >> 4) as usize];
    buf[17] = HEX_CHARS[(mac[4] & 0x0F) as usize];
    buf[18] = HEX_CHARS[(mac[5] >> 4) as usize];
    buf[19] = HEX_CHARS[(mac[5] & 0x0F) as usize];

    buf
}

/// Derives a valid 8-digit Matter passcode (PIN) from a 6-byte MAC address.
pub fn passcode_from_mac(mac: &[u8; 6]) -> u32 {
    let hash = mac_hash(mac);
    let span = (MAX_PASSCODE - MIN_PASSCODE + 1) as u64;
    let mut code = MIN_PASSCODE + ((hash % span) as u32);

    while is_forbidden_passcode(code) {
        code = if code >= MAX_PASSCODE {
            MIN_PASSCODE
        } else {
            code + 1
        };
    }

    code
}

/// Derives a valid 12-bit Matter discriminator from a 6-byte MAC address.
pub fn discriminator_from_mac(mac: &[u8; 6]) -> u16 {
    let hash = mac_hash(mac);
    let mut disc = ((hash >> 32) as u16) & MAX_DISCRIMINATOR;

    if disc == 0 || disc == TEST_DISCRIMINATOR {
        disc = (disc + 1) & MAX_DISCRIMINATOR;
        if disc == 0 {
            disc = MIN_DISCRIMINATOR;
        }
    }

    disc
}

/// Checks whether a passcode is invalid or forbidden by the Matter specification.
pub fn is_forbidden_passcode(passcode: u32) -> bool {
    if passcode < 1 || passcode > MAX_PASSCODE {
        return true;
    }
    FORBIDDEN_PASSCODES.contains(&passcode)
}

/// 64-bit FNV-1a hash over MAC bytes for deterministic distribution.
pub fn mac_hash(mac: &[u8; 6]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for &b in mac {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_valid_passcode_for_known_mac() {
        // Arrange
        let mac = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];

        // Act
        let passcode = passcode_from_mac(&mac);

        // Assert
        assert!(passcode >= MIN_PASSCODE);
        assert!(passcode <= MAX_PASSCODE);
        assert!(!is_forbidden_passcode(passcode));
    }

    #[test]
    fn derives_valid_discriminator_not_equal_to_test_default() {
        // Arrange
        let mac = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];

        // Act
        let discriminator = discriminator_from_mac(&mac);

        // Assert
        assert!(discriminator >= MIN_DISCRIMINATOR);
        assert!(discriminator <= MAX_DISCRIMINATOR);
        assert_ne!(discriminator, TEST_DISCRIMINATOR);
    }

    #[test]
    fn hostname_from_mac_formats_lowercase_suffix() {
        // Arrange
        let mac = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];

        // Act
        let hostname_buf = hostname_from_mac(&mac);
        let hostname = core::str::from_utf8(&hostname_buf).unwrap();

        // Assert
        assert_eq!(hostname, "intercom-bridge-7844");
    }

    #[test]
    fn serial_no_from_mac_formats_expected_prefix_and_hex() {
        // Arrange
        let mac = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];

        // Act
        let serial_buf = serial_no_from_mac(&mac);
        let serial = core::str::from_utf8(&serial_buf).unwrap();

        // Assert
        assert_eq!(serial, "IC-98A316A67844");
    }
}
```

- [ ] **Step 2: Export `commissioning` in `crates/intercom-core/src/domain/mod.rs`**

Add `pub mod commissioning;` to `crates/intercom-core/src/domain/mod.rs`.

- [ ] **Step 3: Update `crates/intercom-adapter-matter/src/commissioning.rs` to re-export and wrap from `intercom-core`**

Update `intercom-adapter-matter::commissioning` to use `intercom_core::domain::commissioning::*`, while keeping `comm_data_from_mac` which constructs `rs_matter::BasicCommData`.

- [ ] **Step 4: Run tests on `intercom-core` to verify host compilation**

Run: `mise exec -- cargo test -p intercom-core --target x86_64-apple-darwin`
Expected: PASS

- [ ] **Step 5: Commit changes**

```bash
git add crates/intercom-core crates/intercom-adapter-matter
git commit -m "refactor: extract pure MAC commissioning logic to intercom-core"
```

---

### Task 2: Create `crates/intercom-pairing-tool` Host Crate

**Files:**
- Create: `crates/intercom-pairing-tool/Cargo.toml`
- Create: `crates/intercom-pairing-tool/src/main.rs`
- Modify: `Cargo.toml` (add to workspace members)

**Interfaces:**
- Consumes:
  - `intercom-core::domain::commissioning::*`
  - `rs_matter::BasicCommData`
  - `rs_matter::pairing::qr::{CommFlowType, DiscoveryCapabilities, Qr, QrPayload, QrTextType, no_optional_data}`
  - `rs_matter::dm::clusters::basic_info::BasicInfoConfig`

- [ ] **Step 1: Create `crates/intercom-pairing-tool/Cargo.toml`**

```toml
[package]
name = "intercom-pairing-tool"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "Host CLI tool for generating Matter commissioning QR and manual pairing codes"

[dependencies]
intercom-core = { path = "../intercom-core" }
rs-matter = { version = "0.4", default-features = false }
```

- [ ] **Step 2: Add `crates/intercom-pairing-tool` to `Cargo.toml` workspace members**

In `Cargo.toml`:
```toml
members = [
    "crates/intercom-core",
    "crates/intercom-adapter-gpio",
    "crates/intercom-adapter-http",
    "crates/intercom-adapter-matter",
    "crates/intercom-adapter-ota",
    "crates/intercom-firmware",
    "crates/intercom-pairing-tool",
]
```

- [ ] **Step 3: Create initial scaffold `crates/intercom-pairing-tool/src/main.rs`**

```rust
//! Matter pairing code & QR generator for Intercom Bridge.

fn main() {
    println!("Intercom Pairing Tool");
}
```

- [ ] **Step 4: Verify host build of `intercom-pairing-tool`**

Run: `mise exec -- cargo check -p intercom-pairing-tool --target x86_64-apple-darwin`
Expected: PASS

- [ ] **Step 5: Commit changes**

```bash
git add Cargo.toml crates/intercom-pairing-tool
git commit -m "feat: scaffold intercom-pairing-tool host crate"
```

---

### Task 3: Implement Matter QR & Manual Code Formatting in `intercom-pairing-tool`

**Files:**
- Modify: `crates/intercom-pairing-tool/src/main.rs`
- Create: `crates/intercom-pairing-tool/tests/pairing_tool_tests.rs`

**Interfaces:**
- Consumes: MAC address (e.g. `98:a3:16:a6:78:44`), derives parameters, prints full visual output.

- [ ] **Step 1: Write integration tests in `crates/intercom-pairing-tool/tests/pairing_tool_tests.rs`**

```rust
use rs_matter::pairing::qr::QrPayload;

#[test]
fn test_qr_payload_round_trip() {
    // Arrange
    let mac = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];
    let passcode = intercom_core::domain::commissioning::passcode_from_mac(&mac);
    let discriminator = intercom_core::domain::commissioning::discriminator_from_mac(&mac);

    let comm_data = rs_matter::BasicCommData {
        password: passcode.to_le_bytes().into(),
        discriminator,
    };

    let dev_det = rs_matter::dm::clusters::basic_info::BasicInfoConfig {
        vid: 0xfff1,
        pid: 0x8001,
        ..rs_matter::dm::clusters::basic_info::BasicInfoConfig::new()
    };

    let payload = QrPayload::new_from_basic_info(
        rs_matter::pairing::DiscoveryCapabilities::BLE,
        rs_matter::pairing::qr::CommFlowType::Standard,
        comm_data,
        &dev_det,
        rs_matter::pairing::qr::no_optional_data as _,
    );

    let mut buf = [0u8; 128];

    // Act
    let (qr_text, _) = payload.as_str(&mut buf).unwrap();

    // Assert
    assert!(qr_text.starts_with("MT:"));
    let parsed = QrPayload::parse_qr_code(qr_text).unwrap();
    assert_eq!(parsed.passcode(), passcode);
    assert_eq!(parsed.discriminator(), discriminator);
    assert_eq!(parsed.vid_pid(), (0xfff1, 0x8001));
}

#[test]
fn test_manual_code_verhoeff_validity() {
    // Arrange
    let mac = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];
    let passcode = intercom_core::domain::commissioning::passcode_from_mac(&mac);
    let discriminator = intercom_core::domain::commissioning::discriminator_from_mac(&mac);

    let comm_data = rs_matter::BasicCommData {
        password: passcode.to_le_bytes().into(),
        discriminator,
    };

    // Act
    let short_code = comm_data.compute_pairing_code();
    let long_code = comm_data.compute_pairing_code_long(0xfff1, 0x8001);

    // Assert
    assert_eq!(short_code.len(), 11);
    assert_eq!(long_code.len(), 21);
    use rs_matter::pairing::qr::QrPayload;
    assert!(QrPayload::parse_pairing_code(&short_code).is_some());
    assert!(QrPayload::parse_pairing_code(&long_code).is_some());
}
```

- [ ] **Step 2: Run test to verify it compiles and passes**

Run: `mise exec -- cargo test -p intercom-pairing-tool --target x86_64-apple-darwin`
Expected: PASS

- [ ] **Step 3: Implement full CLI output formatting in `crates/intercom-pairing-tool/src/main.rs`**

Implement CLI argument parsing (`--mac`, `--json`, `--help`), MAC byte parsing (`parse_mac_address`), device parameter derivation, manual codes, QR text computation, and Unicode half-block QR matrix printing via `Qr::compute` and `qr.line_as_str(QrTextType::Unicode, 4, false, false, y, &mut tmp_buf)`.

- [ ] **Step 4: Test running `intercom-pairing-tool` with `--mac 98:a3:16:a6:78:44`**

Run: `mise exec -- cargo run -p intercom-pairing-tool --target x86_64-apple-darwin -- --mac 98:a3:16:a6:78:44`
Expected: Outputs formatted table with MAC, Passcode, Discriminator, 11-digit & 21-digit manual codes, MT: setup code, and Unicode QR block.

- [ ] **Step 5: Commit changes**

```bash
git add crates/intercom-pairing-tool
git commit -m "feat: implement Matter QR and pairing code generation in intercom-pairing-tool"
```

---

### Task 4: Create Python Launcher Script `scripts/qr.py`

**Files:**
- Create: `scripts/qr.py`

**Interfaces:**
- Auto-detects serial ports via `glob('/dev/cu.usbmodem*') + glob('/dev/cu.usbserial*')`.
- Calls `espflash board-info --port <PORT>`.
- Parses `MAC address: XX:XX:XX:XX:XX:XX`.
- Discovers host target from `rustc -vV`.
- Invokes `cargo run -p intercom-pairing-tool --target <HOST_TARGET> -- --mac <MAC> [EXTRA_ARGS]`.
- Handles flags: `--port`, `--mac`, `--json`.

- [ ] **Step 1: Write `scripts/qr.py`**

Create executable script with:
- `find_serial_port()`
- `get_mac_from_espflash(port)`
- `get_host_target()`
- CLI parser with `--port`, `--mac`, `--json`
- `subprocess.run(["mise", "exec", "--", "cargo", "run", "-p", "intercom-pairing-tool", "--target", host_target, "--", ...])`
- Clear error handling for disconnected USB or busy serial ports.

- [ ] **Step 2: Make `scripts/qr.py` executable**

Run: `chmod +x scripts/qr.py`

- [ ] **Step 3: Test `scripts/qr.py` with manual `--mac` flag**

Run: `python3 scripts/qr.py --mac 98:a3:16:a6:78:44`
Expected: Successfully derives and displays the commissioning codes and QR without requiring USB interrogation.

- [ ] **Step 4: Commit changes**

```bash
git add scripts/qr.py
git commit -m "feat: add scripts/qr.py USB detection and runner script"
```

---

### Task 5: Add Cargo Alias and Documentation

**Files:**
- Modify: `.cargo/config.toml`
- Modify: `AGENTS.md`

**Interfaces:**
- Adds `[alias] qr = "!python3 scripts/qr.py"` to `.cargo/config.toml`.
- Documents `cargo qr` in `AGENTS.md`.

- [ ] **Step 1: Add alias to `.cargo/config.toml`**

```toml
[alias]
qr = "!python3 scripts/qr.py"
```

- [ ] **Step 2: Add `cargo qr` usage section to `AGENTS.md`**

Document:
```markdown
### Viewing Commissioning QR & Pairing Codes

```bash
# Show QR code and pairing codes from connected USB device
cargo qr

# Or manually specify MAC:
cargo qr -- --mac 98:a3:16:a6:78:44

# Or output raw JSON:
cargo qr -- --json
```
```

- [ ] **Step 3: Test running via `cargo qr`**

Run: `cargo qr -- --mac 98:a3:16:a6:78:44`
Expected: Invokes `scripts/qr.py` and prints the output.

- [ ] **Step 4: Commit changes**

```bash
git add .cargo/config.toml AGENTS.md
git commit -m "feat: add cargo qr alias and documentation"
```

---

### Task 6: Hardware End-to-End Verification

**Files:**
- None (verification against physical device)

- [ ] **Step 1: Run `cargo qr` with the connected ESP32-C6 (`/dev/cu.usbmodem14201`)**

Run: `cargo qr`
Expected:
1. Detects `/dev/cu.usbmodem14201`.
2. Reads MAC `98:a3:16:a6:78:44` via `espflash`.
3. Displays Passcode `21710079`, Discriminator `3096`.
4. Displays 11-digit pairing code `3135-671-3252`.
5. Displays `MT:` QR string and Unicode QR matrix.

- [ ] **Step 2: Run all workspace tests on host**

Run: `mise exec -- cargo test -p intercom-core -p intercom-pairing-tool -p intercom-adapter-ota --target x86_64-apple-darwin`
Expected: All tests PASS.

- [ ] **Step 3: Build release firmware to verify embedded target builds cleanly**

Run: `mise exec -- cargo build -p intercom-firmware --release`
Expected: PASS (firmware compiles without error).
