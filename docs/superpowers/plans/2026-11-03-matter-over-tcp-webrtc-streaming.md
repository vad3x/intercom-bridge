# Matter-over-TCP Support for WebRTC Audio Streaming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement inbound Matter-over-TCP on port 5540 to allow Home Assistant (`matter.js`) and Matter 1.5 controllers to negotiate WebRTC SDP offers via Large Messages without connection errors, flashed and automatically verified on the connected ESP32-C6 DevKit board.

**Architecture:** 
- In `rs-matter-stack`, implement `TcpServer<B: TcpBind>` implementing `NetworkSend` and `NetworkReceive` with 4-byte LE framing, chained with UDP in `run_oper_net` via `ChainedNetwork`.
- In `rs-matter-embassy`, wire `edge_nal_embassy::Tcp` into `EnetStack`, allocating a static `TcpSocketBuffers` pool (2 KB RX / 2 KB TX) and incrementing socket headroom.
- In `intercom-matter`, set `tcp_supported: true` in `BasicInfoConfig` to advertise `T=4` in mDNS.
- Flash to the attached ESP32-C6-DevKitM-1 (`/dev/cu.usbmodem14201`) with `partitions-4mb.csv` and run an automated on-device test script verifying mDNS TXT records, TCP port 5540 handshake, and framing roundtrip.

**Tech Stack:** Rust (`no_std`, `riscv32imac-unknown-none-elf`), `embassy-net`, `edge-nal`, `edge-nal-embassy`, `rs-matter`, `rs-matter-stack`, `rs-matter-embassy`, `espflash`, Python 3 (`zeroconf`).

## Global Constraints
- Target hardware: ESP32-C6 (`riscv32imac-unknown-none-elf`), 4 MB DevKit (`--features devkit`, `partitions-4mb.csv`) and 8 MB production.
- Peak call free heap floor: `>= 32 KiB`.
- Buffer sizes: 2,048 bytes RX / 2,048 bytes TX static allocation in `.bss`.
- Concurrency: exactly 1 inbound TCP connection on port 5540.
- Wire framing: 4-byte little-endian message length prefix per Matter Core Spec §4.4.

---

### Task 1: `rs-matter-stack` Generic `TcpServer` Implementation & Unit Tests

**Files:**
- Create: `/Users/vadim/personal/projects/rs-matter-stack/src/tcp.rs`
- Modify: `/Users/vadim/personal/projects/rs-matter-stack/src/lib.rs`
- Test: `/Users/vadim/personal/projects/rs-matter-stack/src/tcp.rs` (inline `mod test`)

**Interfaces:**
- Consumes: `edge_nal::{TcpBind, TcpAccept, TcpSplit}`, `rs_matter::transport::network::{Address, NetworkSend, NetworkReceive}`, `rs_matter::error::Error`
- Produces: `rs_matter_stack::tcp::TcpServer<B>`, and chained TCP/UDP operational transport in `MatterStack::run_oper_net`

- [ ] **Step 1: Write the failing unit tests for `TcpServer` in `rs-matter-stack`**

Create `/Users/vadim/personal/projects/rs-matter-stack/src/tcp.rs` with the unit test asserting 4-byte LE framing, roundtrip read/write, oversized frame rejection, and EOF reconnection:

```rust
#[cfg(test)]
mod test {
    use super::*;
    use edge_nal_std::Stack;

    #[test]
    fn test_tcp_server_framing_roundtrip() {
        // Assert that TcpServer correctly writes 4-byte LE prefix and reads framed packets
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /Users/vadim/personal/projects/rs-matter-stack && cargo test --test tcp test_tcp_server_framing_roundtrip`
Expected: FAIL (module or types not implemented yet)

- [ ] **Step 3: Implement `TcpServer` and wire into `rs-matter-stack/src/lib.rs`**

In `/Users/vadim/personal/projects/rs-matter-stack/src/tcp.rs`:
- Define `TcpServer<B: TcpBind>` holding `Option<Acceptor>`, `Option<(SocketAddr, Reader, Writer)>`.
- Implement `NetworkReceive` for `TcpServer`:
  - `wait_available`: if no connection, await `accept.accept()`. If connected, await `reader.readable()`.
  - `recv_from`: read 4-byte LE length, read exact payload, return `(len, Address::Tcp(peer_addr))`.
  - If length > buffer, close socket and reset to Idle.
  - If EOF, close socket and reset to Idle.
- Implement `NetworkSend` for `TcpServer`:
  - `send_to`: if `addr == Address::Tcp(peer_addr)`, write `(len as u32).to_le_bytes()`, write payload, flush.
In `/Users/vadim/personal/projects/rs-matter-stack/src/lib.rs`:
- Export `pub mod tcp;`
- In `run_oper_net`, if `net_stack.tcp_bind()` returns `Some(tcp_bind)`:
  - Bind `tcp_bind.bind(MATTER_PORT)`
  - Instantiate `TcpServer::new(acceptor)`
  - Split `TcpServer` or use shared reference so `NetworkSend` and `NetworkReceive` can be chained:
    - `ChainedNetwork::new(Address::is_udp, udp_send, tcp_send)`
    - `ChainedNetwork::new(Address::is_udp, udp_recv, tcp_recv)`

- [ ] **Step 4: Run tests in `rs-matter-stack` to verify they pass**

Run: `cd /Users/vadim/personal/projects/rs-matter-stack && cargo test`
Expected: PASS

- [ ] **Step 5: Commit changes in `rs-matter-stack`**

```bash
cd /Users/vadim/personal/projects/rs-matter-stack
git add src/tcp.rs src/lib.rs
git commit -m "feat(tcp): add generic TcpServer with 4-byte LE framing and chain in run_oper_net"
```

---

### Task 2: `rs-matter-embassy` TCP Integration & Buffer Allocation

**Files:**
- Modify: `/Users/vadim/personal/projects/rs-matter-embassy/rs-matter-embassy/src/enet.rs`
- Modify: `/Users/vadim/personal/projects/rs-matter-embassy/rs-matter-embassy/Cargo.toml` (if needed for `edge-nal-embassy/tcp` feature)

**Interfaces:**
- Consumes: `edge_nal_embassy::{Tcp, TcpBuffers, TcpSocketBuffers}`, `embassy_net::Stack`
- Produces: `EnetStack::tcp_bind()` returning `Some(Tcp<'a>)`, `EnetMatterStackResources` with 9 sockets

- [ ] **Step 1: Check existing `EnetStack` and compile error when enabling TCP**

Verify `EnetStack` currently returns `None` for `tcp_bind()`:
`grep -n "fn tcp_bind" /Users/vadim/personal/projects/rs-matter-embassy/rs-matter-embassy/src/enet.rs`

- [ ] **Step 2: Add `TcpBuffers` and wire `Tcp` into `EnetStack`**

In `/Users/vadim/personal/projects/rs-matter-embassy/rs-matter-embassy/src/enet.rs`:
- Define static pool type for Matter TCP buffers:
  ```rust
  pub const ENET_MATTER_TCP_RX_BUF_SIZE: usize = 2048;
  pub const ENET_MATTER_TCP_TX_BUF_SIZE: usize = 2048;
  pub type EnetMatterTcpBuffers = TcpBuffers<1, ENET_MATTER_TCP_RX_BUF_SIZE, ENET_MATTER_TCP_TX_BUF_SIZE>;
  ```
- Increment `ENET_MIN_SOCKET_SET`:
  ```rust
  pub const ENET_MIN_SOCKET_SET: usize =
      ENET_MAX_UDP_SOCKETS + ENET_EXTRA_UDP_SOCKETS + ENET_EXTRA_TCP_SOCKETS + 1 /* Matter TCP */ + 2;
  ```
- Update `EnetStack<'a>` struct to include `tcp: Option<Tcp<'a>>`:
  ```rust
  pub struct EnetStack<'a> {
      udp: Udp<'a>,
      tcp: Option<Tcp<'a>>,
      dns: Dns<'a>,
  }
  ```
- Implement `NetStack::tcp_bind(&self)` returning `self.tcp`.
- Provide `EnetStack::new_with_tcp(stack, udp_buffers, tcp_buffers)`.

- [ ] **Step 3: Compile check `rs-matter-embassy`**

Run: `cd /Users/vadim/personal/projects/rs-matter-embassy/rs-matter-embassy && cargo check`
Expected: PASS

- [ ] **Step 4: Commit changes in `rs-matter-embassy`**

```bash
cd /Users/vadim/personal/projects/rs-matter-embassy
git add rs-matter-embassy/src/enet.rs
git commit -m "feat(enet): enable TCP bind support with 2KB RX/TX buffers for Matter"
```

---

### Task 3: `intercom-matter` Controller mDNS Configuration & Firmware Compilation

**Files:**
- Modify: `crates/intercom-adapter-matter/src/controller.rs`
- Test: Host unit tests & RISC-V release builds

**Interfaces:**
- Consumes: `rs_matter::core::BasicInfoConfig { tcp_supported: true, .. }`
- Produces: mDNS operational TXT record with `T=4`, release firmware binaries for production and devkit

- [ ] **Step 1: Update `BasicInfoConfig` in `intercom-adapter-matter`**

In `crates/intercom-adapter-matter/src/controller.rs`:
Locate `BasicInfoConfig::new()` and set:
```rust
let mut basic_info = BasicInfoConfig::new();
basic_info.tcp_supported = true;
```

- [ ] **Step 2: Run host unit tests**

Run:
```bash
mise exec -- cargo test -p intercom-core -p intercom-adapter-http -p intercom-firmware --lib --target x86_64-apple-darwin
```
Expected: All tests PASS.

- [ ] **Step 3: Build release firmware for production (8 MB)**

Run:
```bash
mise exec -- cargo build -p intercom-firmware --release
```
Expected: Successful compilation, binary output at `target/riscv32imac-unknown-none-elf/release/intercom-bridge`.

- [ ] **Step 4: Build release firmware for DevKit (4 MB)**

Run:
```bash
mise exec -- cargo build-devkit
```
Expected: Successful compilation with `devkit` feature flag.

- [ ] **Step 5: Commit changes in `intercom-matter`**

```bash
git add crates/intercom-adapter-matter/src/controller.rs
git commit -m "feat(matter): enable tcp_supported in BasicInfoConfig for Matter-over-TCP"
```

---

### Task 4: Automated On-Device Flashing & Verification on Attached DevBoard

**Files:**
- Create: `scripts/test_matter_tcp.py`
- Target Board: ESP32-C6-DevKitM-1 on `/dev/cu.usbmodem14201`

**Interfaces:**
- Consumes: Flashed DevKit firmware, local network mDNS discovery, port 5540 socket
- Produces: Automated test report with green checkmarks for mDNS `T=4`, TCP handshake, framing, and clean teardown

- [ ] **Step 1: Write `scripts/test_matter_tcp.py`**

Create `scripts/test_matter_tcp.py` that:
1. Auto-detects device IP on Wi-Fi (via mDNS query `_matter._tcp.local.` or HTTP probe).
2. Verifies mDNS TXT record contains `T=4` (or `T=6`).
3. Connects to `tcp://<device-ip>:5540`:
   - Asserts connection established (`SYN-ACK`).
   - Sends a test 4-byte LE length framed message.
   - Asserts socket remains healthy without device reset.
4. Closes socket and verifies subsequent reconnect succeeds.
5. Prints full diagnostic summary.

- [ ] **Step 2: Flash the connected DevKit board**

Run:
```bash
espflash flash target/riscv32imac-unknown-none-elf/release/intercom-bridge \
    --port /dev/cu.usbmodem14201 \
    --partition-table partitions-4mb.csv
```
Expected: Flash successful, device boots and connects to Wi-Fi.

- [ ] **Step 3: Run the automated test script**

Run:
```bash
python3 scripts/test_matter_tcp.py
```
Expected: All 4 checks PASS (`mDNS T record`, `TCP handshake`, `Framing test`, `Reconnect test`).

- [ ] **Step 4: Live Home Assistant Validation**

In Home Assistant (`/core_matter_server`), open Node 48 Endpoint 1 and click "Listen":
- Verify Home Assistant log shows no `connect ECONNREFUSED ...:5540`.
- Verify `provideOffer` succeeds.
- Verify firmware logs `AudioAV: on_audio_allocated` and `WebRTC: on_offer`.

- [ ] **Step 5: Commit test script**

```bash
git add scripts/test_matter_tcp.py
git commit -m "test: add automated on-device Matter-over-TCP verification script"
```
