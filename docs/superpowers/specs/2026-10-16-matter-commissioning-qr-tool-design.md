# Matter Commissioning QR & Pairing Code Tool Design Spec

## 1. Overview

This document specifies the design for a developer tool (`cargo qr` / `scripts/qr.py`) that extracts the hardware MAC address from a USB-connected ESP32-C6 Intercom Bridge and displays the Matter commissioning QR code and manual pairing codes in the terminal.

The tool uses `rs-matter` directly to compute:
- 11-digit manual pairing code (`XXXX-XXX-XXXX`)
- 21-digit manual pairing code with VID/PID (`XXXX-XXX-XXXX-XXXXX-XXXXX`)
- Matter setup QR code payload (`MT:...`)
- High-contrast 2D Unicode half-block QR code for instant scanning from terminal via iOS Apple Home, Google Home, or other Matter commissioners.

---

## 2. Architecture & Components

```
┌────────────────────────────────────────────────────────┐
│                   Developer Invocation                 │
│          `cargo qr [args]`  OR  `scripts/qr.py`         │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│            Launcher Script: `scripts/qr.py`            │
│  - Detects USB ports (/dev/cu.usbmodem*, usbserial*)   │
│  - Runs `espflash board-info` to read chip eFuse MAC   │
│  - Queries host target (e.g. x86_64 / aarch64 darwin)  │
│  - Invokes `intercom-pairing-tool` with host target    │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│       Host Binary: `crates/intercom-pairing-tool`      │
│  - Workspace member crate compiled for host            │
│  - Pure domain math from `intercom-core`               │
│  - Official QR & Pairing code engine from `rs-matter`  │
│  - Prints scannable Unicode QR & manual codes          │
└────────────────────────────────────────────────────────┘
```

### 2.1 Workspace Member: `crates/intercom-pairing-tool`
A dedicated host Rust binary crate:
- **`Cargo.toml`**:
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
- **Inputs**: `--mac <MAC_STR>` (or raw MAC via args), optional `--json`.
- **Outputs**: Formatted terminal output or JSON with device identity, pairing codes, and Unicode QR block.

### 2.2 Shared Domain Logic in `crates/intercom-core`
Extract the hardware-agnostic MAC derivation algorithms from `intercom-adapter-matter::commissioning` into `intercom-core::domain::commissioning`:
- `mac_hash(mac: &[u8; 6]) -> u64`
- `passcode_from_mac(mac: &[u8; 6]) -> u32`
- `discriminator_from_mac(mac: &[u8; 6]) -> u16`
- `serial_no_from_mac(mac: &[u8; 6]) -> [u8; 15]`

`intercom-adapter-matter` re-exports or consumes these methods from `intercom-core`. This guarantees 100% algorithm parity between device firmware and the host tool without code duplication.

### 2.3 Launcher Script: `scripts/qr.py`
A lightweight Python 3 script following the conventions of `scripts/ota.py` and `scripts/monitor.py`:
- Auto-detects serial ports using `glob('/dev/cu.usbmodem*') + glob('/dev/cu.usbserial*')`.
- Runs `espflash board-info --port <PORT>` to retrieve the hardware MAC address.
- Allows overriding via `--port <PORT>` or `--mac <MAC>`.
- Auto-detects host target triple using `rustc -vV` (e.g. `x86_64-apple-darwin` or `aarch64-apple-darwin`).
- Invokes `cargo run -p intercom-pairing-tool --target <HOST_TARGET> -- <ARGS>`.

### 2.4 Cargo Integration: `.cargo/config.toml`
Add an alias to `.cargo/config.toml`:
```toml
[alias]
qr = "!python3 scripts/qr.py"
```
Developers can run either `cargo qr` or `python3 scripts/qr.py`.

---

## 3. Data Flow & Calculations

1. **MAC Detection**:
   `scripts/qr.py` runs `espflash board-info --port <PORT>` and regex-matches `MAC address:\s+([0-9a-fA-F:]{17})`.
2. **Parameters Derivation**:
   - `mac`: 6 bytes `[0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44]`
   - `passcode`: 8-digit number derived via 64-bit FNV-1a hash over MAC bytes, filtered for forbidden passcodes (`21710079`).
   - `discriminator`: 12-bit number derived via hash (`3096` / `0x0C18`).
   - `serial_no`: formatted as `IC-XXXXXXXXXXXX` (`IC-98A316A67844`).
3. **Matter Manual Pairing Code**:
   - Standard 11-digit pairing code computed with `BasicCommData::compute_pretty_pairing_code()` (`3135-671-3252`).
   - Long 21-digit pairing code computed with `BasicCommData::compute_pretty_pairing_code_long(0xfff1, 0x8001)` containing vendor and product IDs.
4. **Matter QR Payload**:
   - Built with `rs_matter::pairing::qr::QrPayload::new_from_basic_info`:
     - `discovery_capabilities`: `DiscoveryCapabilities::BLE` (`0x02`)
     - `comm_flow`: `CommFlowType::Standard` (`0`)
     - `vid`: `0xFFF1`
     - `pid`: `0x8001`
   - Encoded to standard Base38 format starting with `MT:`.
   - Rendered using `Qr::compute` and `qr.lines_range(QrTextType::Unicode, 4)` to generate the terminal block characters.

---

## 4. Terminal Output Format

```text
================================================================================
  Smart Intercom — Matter Commissioning Code
================================================================================

  Device MAC:          98:a3:16:a6:78:44
  Serial Number:       IC-98A316A67844
  Vendor ID / PID:     0xFFF1 (65521) / 0x8001 (32769)
  Discriminator:       3096 (0x0C18)
  Passcode (PIN):      21710079

  Manual Pairing Code (Standard 11-digit):
  ┌─────────────────┐
  │  3135-671-3252  │
  └─────────────────┘

  Manual Pairing Code (Long 21-digit with VID/PID):
  ┌───────────────────────────────┐
  │  7135-671-3252-65521-32769-X  │
  └───────────────────────────────┘

  Matter Setup QR Code:
  MT:Y.K9042C00KA0648G00

  Scan with Apple Home / Google Home / SmartThings:

  █████████████████████████████████
  █████████████████████████████████
  ████ ▄▄▄▄▄ █▀▄ █ ▄▄█ ▄▄▄▄▄ ████
  ████ █   █ █ █▀█ █ █ █   █ ████
  ████ █▄▄▄█ █▀▀ █▀█▄█ █▄▄▄█ ████
  ████ ▄▄▄▄▄ █▄█▄█▄█▄█ ▄▄▄▄▄ ████
  ████ █▄█ █ ▄ ▀▀▀█ ▄█ ▄▄ ▄▀ ████
  ...
  █████████████████████████████████
  █████████████████████████████████
```

---

## 5. Error Handling

- **No device plugged in**: Prints a friendly error indicating no serial port was found, and shows instructions to specify `--mac <MAC>`.
- **Multiple USB devices**: Lists matching ports and prompts the user to select one or supply `--port <PORT>`.
- **Serial port busy**: Catches `espflash` connection errors and suggests freeing the port from active monitor processes.
- **Malformed MAC address**: Validates string formatting (`AA:BB:CC:DD:EE:FF` or `AABBCCDDEEFF`) before processing.

---

## 6. Testing & Verification

1. **Unit Tests in `intercom-core`**:
   - `test_mac_hash_deterministic`: validates FNV-1a hash output for test vectors.
   - `test_passcode_derivation_bounds_and_forbidden_filter`: ensures derived passcodes stay within `10_000_001..=99_999_998` and skip forbidden codes.
   - `test_discriminator_derivation`: validates 12-bit range and test default exclusion (`3840`).
2. **Integration Tests in `intercom-pairing-tool`**:
   - `test_qr_payload_round_trip`: generates QR string for test MAC `[0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44]`, parses it using `rs_matter::pairing::qr::QrPayload::parse_qr_code()`, and verifies matching discriminator and passcode.
   - `test_manual_code_verhoeff`: verifies that generated manual pairing codes pass `validate_verhoeff_check_digit()`.
3. **Live Hardware Verification**:
   - Run `cargo qr` against connected ESP32-C6 on `/dev/cu.usbmodem14201`.
   - Verify MAC, codes, and camera scannability.
