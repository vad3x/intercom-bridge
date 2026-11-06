# Design: Upstream Production PRs for rs-matter and rs-matter-embassy

## Context

The Intercom Bridge project developed several core Matter 1.4 capabilities and transport optimizations on local fork branches. Upstream has recently refactored `project-chip/rs-matter` into a modular workspace (`rs-matter-dm`, `rs-matter-dm-clusters`, `rs-matter-transport`, `rs-matter-stack`), and merged the separate `rs-matter-stack` repository in-tree.

To upstream our changes cleanly into `project-chip/rs-matter` and `sysgrok/rs-matter-embassy`, the code must adhere to upstream coding conventions, `no_std` zero-cost abstractions, strict Matter specification compliance (Matter 1.4 Endpoint 1 Audio Doorbell `0x0141`, Cluster `0x0551`, global attributes `0xFFF8`–`0xFFFD`), and defensive resource bounds suitable for microcontrollers.

## Goals / Non-Goals

**Goals:**
- **Modular Upstream Alignment**: Place changes into their respective upstream monorepo crates (`rs-matter-dm-clusters`, `rs-matter-transport`, `rs-matter-stack`, `rs-matter-embassy`).
- **Zero-Allocation Data Model**: Implement `AudioAvStreamHandler` with fixed-capacity arrays, stack-allocated TLV builders, and zero dynamic heap allocation in steady state.
- **Microcontroller SRAM Safety**: Reduce baseline memory consumption via on-demand TCP socket buffers (saving 16–24 KiB when inactive) and stack budgeting suitable for 48 KiB execution stacks.
- **Specification Compliance**: Pass Matter standard tests for Audio Doorbell `0x0141`, Camera AV Stream Management `0x0551`, and Matter-over-TCP `T=6` operational discovery.
- **Test Automation**: Provide unit tests covering all TLV encoders/decoders, stream ID wrapping, reference count saturation, and embedded TCP framing and lifecycle edge cases.

**Non-Goals:**
- Full Camera Video / Snapshot handling (H.264/H.265/MJPEG). Video clusters belong in separate PRs; this design isolates the audio doorbell profile.
- Multiplexed multi-client TCP transport on embedded targets. The embedded listener supports 1 active operational TCP session.
- Altering the domain access control boundary in `intercom-core` (audio streaming never actuates the strike relay).

## Decisions

### 1. Dedicated `AudioAvStreamHandler` vs Monolithic Camera Handler
- **Decision**: Introduce a dedicated `AudioAvStreamHandler` in `rs-matter-dm-clusters` (or `rs-matter-dm`) specifically exposing `Feature::AUDIO | Feature::SPEAKER` without video/snapshot state.
- **Rationale**: A full Matter 1.4 camera cluster includes video resolutions, frame rates, snapshot storage, and optical sensor params. Instantiating these data structures consumes over 4 KiB of SRAM per endpoint, which is wasted on audio doorbells. An audio-only handler provides full compliance for `DEV_TYPE_AUDIO_DOORBELL` (`0x0141`) with negligible RAM footprint.
- **Alternatives Considered**:
  - *Monolithic handler with Option fields*: Higher code complexity and runtime overhead; still wastes memory in static descriptors.
  - *IDL-generated stub*: Fails to provide command handling for `AllocateAudioStream` and `DeallocateAudioStream`.

### 2. Stream ID Wrapping and Reference Count Guards
- **Decision**: Implement monotonic `stream_id` allocation wrapping from `0xFFFF` to `1` (skipping in-use IDs) and explicit `u8` reference counting guarded against overflow (`saturating_add` or explicit `ResourceExhausted` check).
- **Rationale**: Matter controllers (like Home Assistant or Apple Home) may open and close audio streams repeatedly during visitor calls. If stream IDs collide or fail to clean up during active WebRTC leases, streaming breaks or crashes.
- **Alternatives Considered**: Simple incrementing counter without wrapping check (fails after 65,535 stream calls); unbounded reference counts.

### 3. Matter-over-TCP Framing and Embedded Stack Integration
- **Decision**: Use Matter's 4-byte little-endian length prefix. Preserve the existing host `TcpNetwork` framing in `rs-matter-transport`; implement or reuse the framing logic needed by the `no_std` embedded listener in `rs-matter-stack`.
- **Rationale**: The CHIP reference transport and current upstream `rs-matter-transport` both use a 32-bit little-endian length. The existing host transport already covers framing, fragmented input, and back-to-back frames, so PR 2 should focus on the missing embedded listener and its integration with the stack's UDP runner.
- **Alternatives Considered**: A 2-byte big-endian prefix (incompatible with Matter peers); duplicating framing logic in multiple transports without shared tests.

### 4. Non-Blocking Cooperative Polling in `rs-matter-stack`
- **Decision**: In `rs-matter-stack`, run the TCP listener and stream handling cooperatively with UDP MRP using non-blocking async futures (`select` / `join`).
- **Rationale**: Matter relies on UDP MRP for discovery and fast commands. If the TCP read/write loop blocks on slow socket drains or TLS handshakes, UDP retransmissions timeout and drops the device off the fabric.
- **Alternatives Considered**: Dedicated OS thread (unavailable on `no_std` single-threaded Embassy runtimes).

### 5. Operational mDNS TCP Capability
- **Decision**: Set the operational device details' `tcp_supported` flag when the embedded TCP server is enabled, reusing the existing mDNS TXT encoder. The `T=6` bitmap combines TCP client (`2`) and TCP server (`4`) support.
- **Rationale**: The upstream transport already encodes `T=6` from `tcp_supported`; stack integration must advertise it only when the configured device can accept operational TCP connections.
- **Alternatives Considered**: Reimplementing the mDNS TXT record or advertising TCP server support unconditionally.

### 6. On-Demand Embassy TCP Buffers
- **Decision**: In `sysgrok/rs-matter-embassy`, allocate TCP socket RX and TX buffers on demand when a connection is established, returning them upon close.
- **Rationale**: Preserves 16–24 KiB of SRAM during normal idle standby, leaving critical headroom for Opus audio encoding and Wi-Fi drivers on ESP32-C6.
- **Alternatives Considered**: Static buffer reservation (leaves only ~27 KiB stack on ESP32-C6, risking stack overflow during voiced Opus encoding).

## Risks / Trade-offs

- **[Risk: Upstream Maintainer Pushback on Non-Codegen Cluster Handler]**
  - *Mitigation*: Upstream `rs-matter` uses hand-crafted, high-efficiency handlers for performance-sensitive clusters (e.g. `basic_info`, `wifi_diag`). Document that `AudioAvStreamHandler` faithfully implements the exact IDL cluster definition and passes all standard TLV decode/encode unit tests.
- **[Risk: Merge Conflicts with In-Flight Upstream Refactorings]**
  - *Mitigation*: Base each PR directly on the latest `upstream/main` commit and use git cherry-pick / format-patch against the new crate boundaries (`rs-matter-dm-clusters` and `rs-matter-stack`).
- **[Risk: TCP Socket Exhaustion on Multi-Controller Fabrics]**
  - *Mitigation*: Implement a strict single-connection policy for the embedded TCP listener: if a new connection arrives while one is active, gracefully refuse or close the new attempt without terminating the active call.

## Migration Plan

1. **Step 1 (PR 1)**: Submit `feat(dm): add DEV_TYPE_AUDIO_DOORBELL (0x0141) and AudioAvStreamHandler (0x0551)` to `project-chip/rs-matter`.
2. **Step 2 (PR 2)**: Submit `feat(transport/stack): add embedded Matter-over-TCP listener and operational runner` to `project-chip/rs-matter`.
3. **Step 3 (PR 3)**: Submit `feat(enet): on-demand TCP buffer allocation & network sizing features` to `sysgrok/rs-matter-embassy`.
4. **Step 4 (Intercom Bridge Cleanup)**: Once merged or tagged upstream, update `Cargo.toml` in `intercom-matter` to consume upstream crates, removing local path patches.
