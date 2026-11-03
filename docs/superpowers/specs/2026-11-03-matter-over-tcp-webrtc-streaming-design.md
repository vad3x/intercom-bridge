# Matter-over-TCP Support for WebRTC Audio Streaming

- **Date:** 2026-11-03
- **Status:** Approved Design — Ready for Implementation Planning
- **Problem Reference:** [docs/problem-matter-over-tcp-webrtc-streaming.md](../../problem-matter-over-tcp-webrtc-streaming.md)
- **Parent Spec:** [2026-10-30-v2-slice-4-matter-audio-doorbell-design.md](2026-10-30-v2-slice-4-matter-audio-doorbell-design.md)
- **Affected Repositories:**
  - `rs-matter-stack` (`/Users/vadim/personal/projects/rs-matter-stack`)
  - `rs-matter-embassy` (`/Users/vadim/personal/projects/rs-matter-embassy`)
  - `intercom-matter` (`/Users/vadim/personal/projects/intercom-matter`)

---

## 1. Executive Summary & Root Cause

When starting a live WebRTC audio stream from Home Assistant's Matter Server UI (`/core_matter_server`, node 48, endpoint 1: `AudioDoorbell` `0x0141`), the handshake allocates an audio stream over UDP, but immediately aborts when delivering the browser's WebRTC SDP offer (~1,000 bytes):
```text
2026-11-03 18:14:08.798 WARN PeerConnection @1:30 tcp://[fe80::...]:5540 General connection error: connect ECONNREFUSED
WebSocket error response (camera_start_stream) [aborted] Operation aborted (Caused by: [peer-unreachable] Peer has been unreachable)
```

### Root Cause
Under the Matter 1.5 Specification (§4.4 and §5.3) and `matter.js` (v0.18.0+), delivering a large SDP payload via `webRtcTransportProvider.provideOffer` is classified as a **`LargeMessage`**, which **mandates a TCP-backed session** over standard Matter port `5540`. Because the bare-metal ESP32-C6 firmware only binds UDP on port 5540, inbound TCP connection attempts are rejected with `ECONNREFUSED`.

This specification establishes the minimal, robust, bare-metal Matter-over-TCP inbound server implementation across `rs-matter-stack`, `rs-matter-embassy`, and `intercom-matter`.

---

## 2. Architecture & Component Boundaries

The implementation strictly preserves Hexagonal Architecture and existing crate responsibilities:

```text
┌────────────────────────────────────────────────────────────────────────┐
│                        intercom-matter (v2)                            │
│  crates/intercom-adapter-matter/src/controller.rs                      │
│  - Configures BasicInfoConfig { tcp_supported: true, .. }              │
│  - mDNS TXT record advertises T=4 (TCP Server) on _matter._tcp.local   │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
┌───────────────────────────────────▼────────────────────────────────────┐
│                        rs-matter-embassy                               │
│  rs-matter-embassy/src/enet.rs                                         │
│  - Implements NetStack::tcp_bind() returning edge_nal_embassy::Tcp     │
│  - Allocates 1 static TcpSocketBuffers (2 KB RX / 2 KB TX) in .bss     │
│  - Increments ENET_MIN_SOCKET_SET by 1 for the Matter TCP listener     │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
┌───────────────────────────────────▼────────────────────────────────────┐
│                         rs-matter-stack                                │
│  rs-matter-stack/src/tcp.rs & lib.rs                                   │
│  - Implements TcpServer<B: TcpBind> (NetworkSend + NetworkReceive)     │
│  - Chains UDP & TCP in run_oper_net via ChainedNetwork:                │
│      Send: ChainedNetwork::new(Address::is_udp, udp_send, tcp_send)    │
│      Recv: ChainedNetwork::new(Address::is_udp, udp_recv, tcp_recv)    │
└────────────────────────────────────────────────────────────────────────┘
```

### Component Roles

1. **`intercom-matter`**:
   - Enables `tcp_supported: true` in `BasicInfoConfig`.
   - Leaves all transport logic to the underlying stack crates.
2. **`rs-matter-embassy`**:
   - Integrates existing `edge_nal_embassy::Tcp` into `EnetStack`.
   - Provides a static `TcpSocketBuffers` pool for 1 socket (`2,048` B RX + `2,048` B TX).
   - Increases socket allocation headroom in `StackResources`.
3. **`rs-matter-stack`**:
   - Introduces generic `TcpServer<B: TcpBind>` implementing `NetworkSend` and `NetworkReceive`.
   - Chains the TCP transport alongside the UDP transport in `run_oper_net`.

---

## 3. Wire Framing, Connection Lifecycle, and Data Flow

### 3.1 Wire Framing (Matter Core Specification §4.4)
Every Matter message transported over TCP is framed with a 4-byte little-endian message length prefix:

```text
┌───────────────────────────┬──────────────────────────────────────────┐
│ Length: u32 (4 bytes, LE) │ Matter Packet Payload (N bytes)          │
└───────────────────────────┴──────────────────────────────────────────┘
```

### 3.2 State Machine & Connection Lifecycle
The server maintains **at most 1 active TCP session** at any time:

```text
  ┌─────────────────────────────────────────────────────────────┐
  │                           Idle                              │
  │                  (Awaiting TCP connection)                  │
  └──────────────────────────────┬──────────────────────────────┘
                                 │ accept() on port 5540
                                 ▼
  ┌─────────────────────────────────────────────────────────────┐
  │                         Connected                           │
  │      peer_addr = Address::Tcp(remote_socket_address)        │
  │      Split socket into: Reader (RX) & Writer (TX)           │
  └──────────────┬──────────────────────────────┬───────────────┘
                 │                              │
     Inbound RX  │                              │ Outbound TX
                 ▼                              ▼
  - Read 4-byte LE length               - Check dest == peer_addr
  - Read exact N payload bytes          - Write 4-byte LE length
  - Yield (N, Address::Tcp(peer))       - Write payload & flush
                 │                              │
                 └──────────────┬───────────────┘
                                │ EOF / Connection Reset / Error
                                ▼
  ┌─────────────────────────────────────────────────────────────┐
  │                    Teardown & Clean Up                      │
  │         Drop active socket, reset state to Idle             │
  └──────────────────────────────┬──────────────────────────────┘
                                 │
                                 └─► Re-enter Idle accept()
```

### 3.3 Full-Duplex Trait Implementations

#### `NetworkReceive`
- `wait_available(&mut self)`:
  - If in `Idle` state: awaits `accept.accept()` on port `5540`. Once accepted, updates state to `Connected(peer_addr, reader, writer)`.
  - If in `Connected` state: awaits `reader.readable()`.
- `recv_from(&mut self, buffer: &mut [u8])`:
  - Reads the 4-byte length prefix: `msg_len = u32::from_le_bytes(...) as usize`.
  - If `msg_len > buffer.len()`, tears down connection (see Section 4).
  - Reads exact `msg_len` bytes into `buffer[..msg_len]`.
  - Returns `Ok((msg_len, Address::Tcp(peer_addr)))`.

#### `NetworkSend`
- `send_to(&mut self, data: &[u8], addr: Address)`:
  - Validates `addr == Address::Tcp(peer_addr)`. If not connected to this peer, returns `Err(ErrorCode::NoNetworkInterface)`.
  - Serializes `(data.len() as u32).to_le_bytes()` followed by `data` to `writer`.
  - Calls `writer.flush()`.

#### Disconnection & Teardown
- When `reader` reads 0 bytes (EOF) or encounters an error, the connection state resets to `Idle`.
- The operational transport runner remains alive; the next `wait_available()` cycle re-enters `accept()`.

---

## 4. Resource Budgeting & Error Handling

### 4.1 Memory & SRAM Budget (ESP32-C6)
- **SDP Payload Sizing**: Browser SDP offers are ~1,000 bytes; with Matter TLV envelope, total message size is ~1,150–1,300 bytes.
- **Buffer Allocation**:
  - RX Buffer: `2,048` bytes (`2 KiB`)
  - TX Buffer: `2,048` bytes (`2 KiB`)
  - Total Static BSS: `4,096` bytes (`4 KiB`)
- **Heap Floor Guarantee**:
  - Buffers are statically allocated in `.bss` and do not consume dynamic heap.
  * Free heap floor during peak WebRTC + Opus audio calls remains comfortably above the **`32 KiB` floor** specified in Slice 3.
- **Socket Headroom**:
  - `ENET_MIN_SOCKET_SET` in `rs-matter-embassy/src/enet.rs` is incremented by 1 (from 8 to 9 total sockets).

### 4.2 Error Handling & Edge Cases
1. **Oversized Frames**:
   - If an incoming length header exceeds `2,048` bytes, the socket is immediately closed and reset to prevent buffer overflow.
2. **Partial Reads & Stalls**:
   - Sockets employ standard read timeouts; stalled partial reads close the connection and reset to `Idle`.
3. **Concurrent Connection Requests**:
   - With 1 active connection, subsequent SYN requests are rejected or queued until the active connection closes.
4. **Network Interface Recovery**:
   - If Wi-Fi link drops, the active TCP socket closes and rebinds port 5540 when network config returns.

---

## 5. Verification & Validation Plan

### Stage 1: Host Unit Tests (macOS)
- In `rs-matter-stack`:
  - Unit test for `TcpServer` with mock/in-memory stream:
    - 4-byte LE framing and deframing verification.
    - Chunked stream assembly.
    - Oversized frame rejection and socket teardown.
    - Clean reconnection after EOF.
- In `intercom-matter`:
  - Run all workspace host tests:
    ```bash
    mise exec -- cargo test -p intercom-core -p intercom-adapter-http -p intercom-firmware --lib --target x86_64-apple-darwin
    ```

### Stage 2: Firmware Compilation
- Build optimized release firmware for ESP32-C6:
  ```bash
  mise exec -- cargo build -p intercom-firmware --release
  ```
- Verify binary size fits within partition boundaries.

### Stage 3: Network & Discovery Verification
- Flash or push firmware via OTA (`scripts/ota.py`).
- Inspect mDNS operational advertisement:
  - Verify TXT record contains `T=4` (or `T=6`).
- Verify TCP connectivity:
  - `nc -zv <intercom-ip> 5540` succeeds.

### Stage 4: Live Home Assistant End-to-End Validation
- Navigate to Home Assistant Matter Server UI (`/core_matter_server`) -> Node 48 -> Endpoint 1.
- Click "Listen" in `webrtc-stream-view`.
- **Pass Criteria**:
  1. `provideOffer` connects to `tcp://<intercom-ip>:5540` without `ECONNREFUSED`.
  2. Device logs confirm `AudioAV: on_audio_allocated` and `WebRTC: on_offer`.
  3. Device generates and returns SDP Answer over TCP.
  4. WebRTC media session establishes over UDP without `Operation aborted` or `Peer has been unreachable`.
  5. Device `/api/status` confirms free heap remains `>= 32 KiB`.
