# Problem Brief: Matter-over-TCP Support Required for WebRTC Audio Streaming

**Date:** 2026-11-03  
**Status:** Open / Needs Specification  
**Affected Repositories:**  
- `intercom-matter` (`/Users/vadim/personal/projects/intercom-matter`)  
- `rs-matter-embassy` (`/Users/vadim/personal/projects/rs-matter-embassy`)  
- `rs-matter` (`/Users/vadim/personal/projects/rs-matter`)  

---

## 1. Executive Summary

When attempting to start an audio stream from Home Assistant's Matter Server ingress dashboard (`/core_matter_server`, node 48, endpoint 1: Audio Doorbell `0x0141`), the stream handshake immediately aborts with:
`[aborted] Operation aborted` (Caused by: `[peer-unreachable] Peer has been unreachable ... connect ECONNREFUSED ...:5540`).

While the initial `cameraAvStreamManagement.audioStreamAllocate` command succeeds over standard UDP/MRP, Home Assistant's `webrtc-stream-view` frontend passes a browser-generated WebRTC SDP Offer (~1,000 bytes) to `camera_start_stream`. In the Matter 1.5 specification and `matter.js` (v0.18.0+), delivering a large SDP offer via `webRtcTransportProvider.provideOffer` is classified as a **`LargeMessage`**, which **mandates a TCP-backed session** over port 5540.

Because the bare-metal ESP32-C6 firmware (`rs-matter-embassy`) only binds UDP on port 5540, the TCP connection attempt from Home Assistant is rejected with `ECONNREFUSED`. Matter Server marks the node unreachable over TCP, aborts the stream setup, and rolls back the allocated audio stream.

---

## 2. Verified Diagnostic Traces & Evidence

### A. Home Assistant Matter Server Log (`/core_matter_server`)
```text
2026-11-03 18:13:55.946 INFO  ClientInteraction    Invoke » @1:30•f6ca⇵e7ec 1.cameraAvStreamManagement.audioStreamAllocate with streamUsage: 3 audioCodec: 0 channelCount: 1 sampleRate: 48000 bitRate: 64000 bitDepth: 16
2026-11-03 18:13:56.003 INFO  ClientInteraction    Invoke « @1:30•f6ca⇵e7ec 1.cameraAvStreamManagement.audioStreamAllocate audioStreamId: 10
2026-11-03 18:13:56.008 INFO  ClientInteraction    Read » @1:30•f6ca⇵e7ed attributes: 1.cameraAvStreamManagement.*
2026-11-03 18:13:56.064 INFO  ClientInteraction    Read « @1:30•f6ca⇵e7ed attributes: 1 events: 0
2026-11-03 18:13:56.080 INFO  ClientInteraction    Invoke » @1:30•f6ca⇵e7ee 1.cameraAvStreamManagement.audioStreamDeallocate with audioStreamId: 10
2026-11-03 18:13:56.129 INFO  ClientInteraction    Invoke « @1:30•f6ca⇵e7ee 1.cameraAvStreamManagement.audioStreamDeallocate status: Success (0)
2026-11-03 18:13:56.134 INFO  ClientInteraction    Read » @1:30•f6ca⇵e7ef attributes: 1.cameraAvStreamManagement.*
2026-11-03 18:13:56.177 INFO  ClientInteraction    Read « @1:30•f6ca⇵e7ef attributes: 1 events: 0
2026-11-03 18:13:56.182 ERROR WebSocketC~erHandler [0006] WebSocket error response (camera_start_stream) 1396280805 [aborted] Operation aborted
    at abort.throwIfAborted (/app/node_modules/@matter/general/src/util/Abort.ts:215:21)
    at Peer.connect (/app/node_modules/@matter/protocol/src/peer/Peer.ts:490:24)
    at process.processTicksAndRejections (node:internal/process/task_queues:104:5)
    at async PeerExchangeProvider.connect (/app/node_modules/@matter/protocol/src/peer/PeerExchangeProvider.ts:59:9)
    at async PeerExchangeProvider.initiateExchange (/app/node_modules/@matter/protocol/src/peer/PeerExchangeProvider.ts:83:17)
    at async InteractionClientMessenger.create (/app/node_modules/@matter/protocol/src/interaction/InteractionMessenger.ts:1098:26)
    at async ClientInteraction.#begin (/app/node_modules/@matter/protocol/src/action/client/ClientInteraction.ts:1249:29)
    at async ClientInteraction.#invokeSingle (/app/node_modules/@matter/protocol/src/action/client/ClientInteraction.ts:488:31)
  Caused by: [peer-unreachable] Peer has been unreachable for 11m 47s
    at abort.timeoutHandler (/app/node_modules/@matter/protocol/src/peer/Peer.ts:483:27)
    at abort.timeoutHandler (/app/node_modules/@matter/general/src/util/Abort.ts:102:34)
    at <anonymous> (/app/node_modules/@matter/general/src/util/Abort.ts:116:45)
    at process.processTicksAndRejections (node:internal/process/task_queues:104:5)
2026-11-03 18:14:08.798 WARN  PeerConnection       @1:30 tcp://[fe80::9e13:9eff:fed7:bda8%end0]:5540 General connection error (retry in 2m): connect ECONNREFUSED fe80::9e13:9eff:fed7:bda8%end0:5540
  at TCPConnectWrap.afterConnect [as oncomplete] (node:net:2083:16)
2026-11-03 18:14:53.949 WARN  PeerConnection       @1:30 tcp://192.168.0.101:5540 General connection error (retry in 2m): connect ECONNREFUSED 192.168.0.101:5540
  at TCPConnectWrap.afterConnect [as oncomplete] (node:net:2083:16)
```

### B. Home Assistant Browser Ingress Client Messages
```json
// Browser initiates camera_start_stream with SDP:
{
  "message_id": "1396280805",
  "command": "camera_start_stream",
  "args": {
    "node_id": 48,
    "endpoint_id": 1,
    "stream_usage": "LiveView",
    "sdp": "v=0\r\no=- 8622171157340920873 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\na=group:BUNDLE 0\r\na=extmap-allow-mixed\r\na=msid-semantic: WMS\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111 63 9 0 8 13 110 126\r\n...",
    "video": false,
    "talkback": true,
    "client_session_key": "webrtc-stream-view-1"
  }
}

// Browser trickles ICE candidate:
{
  "message_id": "1396280806",
  "command": "camera_provide_ice_candidates",
  "args": {
    "node_id": 48,
    "endpoint_id": 1,
    "client_session_key": "webrtc-stream-view-1",
    "ice_candidates": [{"candidate": "candidate:2607137529 1 udp ...", "sdpMid": "0", "sdpMLineIndex": 0}]
  }
}

// Server returns abort and missing session errors:
{"message_id": "1396280805", "error_code": 0, "details": "Operation aborted"}
{"message_id": "1396280806", "error_code": 8, "details": "client_session_key \"webrtc-stream-view-1\" names no session of this connection that is starting or open on node @?:30 endpoint 1"}
```

### C. Device Firmware Log (`http://192.168.0.101:8080/api/logs`)
```text
00:03:55.485 INFO  intercom_adapter_matter::audio_doorbell - AudioAV: on_audio_allocated stream_id=3, codec=OPUS, rate=48000, bitrate=64000
00:03:55.582 INFO  intercom_adapter_matter::audio_doorbell - AudioAV: on_audio_deallocated stream_id=3
```
*Note: `WebRTC: on_solicit_offer` and `WebRTC: on_offer` were never reached because the command could not be transported across the network.*

---

## 3. Protocol & Specification Analysis

1. **Matter 1.5 Media Signaling Transport Rules**:
   - The Matter Message Reliability Protocol (MRP) over UDP operates within standard IPv6 MTU limits (1,280 bytes), leaving approximately 1,100 bytes for application payloads.
   - WebRTC SDP offers and answers typically range from 900 bytes to several kilobytes.
   - The Matter 1.5 Core Specification specifies that **Matter-over-TCP** on standard port `5540` is used for Large Message payloads, including SDP negotiation commands on `WebRtcTransportProvider` (`0x0553`) and `WebRtcTransportRequestor` (`0x0554`).

2. **`matter.js` v0.18.0 Implementation Details**:
   - Starting with v0.18.0, `ClientRequest.largeMessage` explicitly requires a TCP-backed operational session.
   - `webRtcTransportProvider.provideOffer` is marked with `largeMessage: true`.
   - When attempting to invoke `provideOffer`, `ClientInteraction.#begin` checks for an active TCP session. If none exists, it triggers `Peer.connect` targeting `tcp://<ip>:5540`.
   - If the device returns TCP RST (`ECONNREFUSED`), the peer is flagged as unreachable, triggering the abort and rollback.

3. **mDNS Announcement Requirements**:
   - Matter nodes supporting TCP must advertise the `T` key in their mDNS operational TXT records (`_matter._tcp.local`):
     - `T` bitmap: `bit 1 (0x02)` = TCP client, `bit 2 (0x04)` = TCP server.
     - A device accepting inbound TCP connections advertises `T=6` (or `T=4`).
     - Current device setting is `tcp_supported: false`, omitting the `T` record.

---

## 4. Current State Across the Workspace

### Repository Status:
| Component | Repository | File Path | Current Status |
|---|---|---|---|
| Core TCP Framing & Protocol | `rs-matter` | `rs-matter/src/transport/network/tcp.rs` | Implemented for Unix/Windows `std` using `async_io::Async<std::net::TcpListener>`. Framed with 4-byte LE length. |
| Operational Preference | `rs-matter` | `rs-matter/src/transport.rs` | Has `TransportPreference::LargePayload` and `Address::Tcp(addr)`. |
| Bare-metal Embassy Network | `rs-matter-embassy` | `rs-matter-embassy/src/enet.rs` | **Unimplemented**. Stubbed with `// TODO: Implement optional TCP support with a feature flag`, returning `None` for `tcp_bind()` and `tcp_connect()`. |
| Device Configuration | `intercom-matter` | `crates/intercom-adapter-matter/src/controller.rs` | `dev_det` sets `BasicInfoConfig { ..BasicInfoConfig::new() }`, which defaults to `tcp_supported: false` (no `T` in mDNS). |
| Application HTTP Server | `intercom-matter` | `crates/intercom-adapter-http/src/server.rs` | Currently consumes 2 TCP sockets from `embassy-net` on port 8080. |

---

## 5. Scope of Work for the Implementation Spec

An agent or developer writing the implementation specification will need to address the following components:

### 1. `rs-matter-embassy`: `no_std` `embassy-net` TCP Transport
- Implement `TcpBind` and `TcpConnect` using `embassy_net::tcp::TcpSocket`.
- Bind TCP port `5540` alongside existing UDP socket.
- Manage connection lifecycle:
  - Handle inbound connection handshake.
  - Implement 4-byte little-endian message framing.
  - Dispatch framed packets to `rs-matter`'s `NetworkReceive` / Interaction Model.
  - Serialize outbound responses through `NetworkSend` for `Address::Tcp`.
  - Handle disconnection and connection cleanup without leaking socket resources.

### 2. Memory & Socket Budgeting on ESP32-C6
- ESP32-C6 has limited internal SRAM (~512 KB, with ~80-100 KB free at runtime in this firmware).
- `embassy-net` requires dedicated RX and TX packet buffers per TCP socket:
  - Determine optimal buffer size (e.g. 2 KB RX / 2 KB TX per Matter TCP socket).
  - Increase `ENET_MAX_TCP_SOCKETS` in `rs-matter-embassy/src/enet.rs` without starving the existing HTTP server (port 8080) or WebRTC media pipeline.

### 3. Controller & Identity Configuration (`intercom-matter`)
- Update `crates/intercom-adapter-matter/src/controller.rs` to set `tcp_supported: true` in `BasicInfoConfig`.
- Verify that mDNS broadcasts TXT key `T=6` on `<compressed_fabric>-<node_id>._matter._tcp.local`.

### 4. Verification & Testing
- Unit/host tests in `rs-matter` and `rs-matter-embassy`.
- Build verification for `riscv32imac-unknown-none-elf` target.
- Live integration verification: Click "Listen" in Home Assistant Matter Server and verify that:
  - `provideOffer` connects to TCP:5540 and delivers the SDP offer.
  - Firmware logs `WebRTC: on_offer` or `on_solicit_offer`.
  - Session establishes successfully without `Operation aborted` or `ECONNREFUSED`.
