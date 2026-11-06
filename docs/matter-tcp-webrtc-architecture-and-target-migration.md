# Architecture: Matter-over-TCP, WebRTC Audio, and Next-Gen Target Migration

**Status:** Implemented & Verified in `feat/matter-over-tcp`  
**Targets Evaluated:** ESP32-C6 (Current) → ESP32-C61 / ESP32-S3 (Next-Gen)  
**Last Updated:** November 2026

---

## 1. Executive Summary

This document describes the architectural implementation of **Matter-over-TCP** and **bidirectional WebRTC audio streaming** in the Smart Intercom firmware, the memory constraints discovered on the ESP32-C6, the resolution to the call-stack overflow issue, and why migrating to a target with PSRAM support (such as **ESP32-C61** or **ESP32-S3**) fundamentally resolves the platform's trade-offs.

---

## 2. Architecture: Matter 1.5 Media Signaling & WebRTC

```
+---------------------------------------------------------------------------------+
|                                 Apple Home / HA                                 |
+---------------------------------------------------------------------------------+
          |                                                   |
 (Large SDP > 1 KiB)                                  (RTP/SRTP Audio)
          |                                                   |
          v (TCP:5540)                                        v (UDP:Dynamic)
+------------------------+                          +-----------------------------+
| rs-matter-stack (TCP)  |                          | intercom-adapter-webrtc     |
| - mDNS TXT "T=6"       |                          | - ICE-Lite Agent            |
| - Framing / LargeMsg   |                          | - DTLS 1.2 + RFC 5764 SRTP  |
+------------------------+                          | - HalfDuplexOpusCodec       |
          |                                         +-----------------------------+
          v                                                   ^
+------------------------------------+                        |
| intercom-adapter-matter            |                        |
| - CameraAVStreamManagement (0x0551)|------------------------+
| - WebRtcTransportProvider (0x0553) |  MediaSessionPort
+------------------------------------+
```

### Key Components

1. **Dual-Stack Transport (`rs-matter-stack`)**:
   - Port 5540 concurrently binds both **UDP** (standard Matter message reliability protocol, MRP) and **TCP** (Matter-over-TCP framing with 2-byte length prefixes).
   - mDNS announcements set `T=6` in TXT records, notifying ecosystem controllers that the device accepts TCP connections.
   - Enables transfer of browser-generated SDP offers (~1,000–1,500 bytes) which exceed MRP packet MTU.

2. **WebRTC Media Engine (`intercom-adapter-webrtc`)**:
   - **ICE-Lite Agent**: Handles STUN Binding requests directly over UDP without external STUN/TURN servers.
   - **DTLS-SRTP Handshake**: Performs TLS 1.2 handshake over UDP, verifies peer fingerprint, and derives 128-bit AES-GCM SRTP session keys via RFC 5764.
   - **Audio Pipeline**:
     - *Listen Mode*: 16 kHz bus PCM downsampled to 8 kHz, encoded via libopus VoIP mode into 20 ms frames (50 packets/sec).
     - *Talk Mode*: Inbound RTP packets decrypted via SRTP and decoded to 16 kHz PCM.
     - *Half-Duplex Interlock*: Enforces software turn-taking with hangover timers to prevent audio feedback.
   - **Shared Session Engine**: A single `WebRtcMediaEngine` instance serves both Matter Endpoint 1 and the embedded Web Dashboard on port 8080.

---

## 3. ESP32-C6 Resource Realities & Tuning

### Memory Layout on ESP32-C6

The ESP32-C6 has 512 KiB total SRAM. However, after deducting hardware ROM allocations, cache memory, Wi-Fi 6 PHY buffers, and BLE controller memory, the firmware is left with **~320 KiB of usable DRAM (`RWDATA`)**.

In `esp-hal`, memory in `RWDATA` is allocated strictly in this order:
1. `.data` (initialized static variables)
2. `.bss` (uninitialized static variables + heap buffer)
3. `.stack` (whatever memory remains until the end of `RWDATA`)

### The 3-Second "Listen" Crash: Analysis & Root Cause

When running live WebRTC streaming, the device repeatedly crashed after ~3 seconds of active audio transmission with:
```text
Stack overflow detected in 0x40821060.
Stack pointer: 408679a0, Task stack range: 408679a0 ..= 4086e610
```

**Backtrace:**
`opus_encode` → `silk_Encode` → `silk_encode_frame_FIX` → `silk_find_pitch_lags_FIX` → `silk_pitch_analysis_core`.

**Why it occurred:**
1. `HEAP_SIZE` was set to `140 KiB` in `.bss`. This left only **27,760 bytes** (27.1 KB) for the main task `.stack`.
2. For the first ~2 seconds, audio was silence/DTX. When voiced audio entered the microphone, SILK fixed-point pitch search engaged (`silk_pitch_analysis_core`), which uses recursive calls with variable-length arrays (`VAR_ARRAYS`).
3. The stack pointer reached `408679a0` (the exact bottom of the task stack), triggering `esp-rtos`'s hardware/software stack guard assertion.
4. Concurrently, heap telemetry showed `free_heap_bytes=41292, min_free_heap_bytes=32032` — meaning **32 KiB of heap was completely unused and wasted**, while the stack was starved.

### The ESP32-C6 Fix: Rebalanced Memory Pool

In `crates/intercom-firmware/src/config.rs`:
- Tuned `HEAP_SIZE` from `140 KiB` to **`120 KiB`**.
- This immediately expanded `.stack` from **`27,760 bytes` (27.1 KB) → `48,240 bytes` (47.1 KB)**.
- Provides >20 KiB of additional call stack margin while preserving ~12 KiB of guaranteed free heap headroom at peak load.

---

## 4. Current Disadvantages & Bottlenecks of ESP32-C6

While the ESP32-C6 is currently operational with these tunings, it represents the absolute ceiling of the silicon:

| Dimension | ESP32-C6 Limitation | Impact on Firmware |
|---|---|---|
| **Memory** | 512 KiB SRAM, **no external PSRAM** | Heap and stack must be manually tuned down to within 5-10 KiB margins. Zero room for large audio buffers or acoustic echo cancellation (AEC). |
| **CPU** | Single 32-bit RISC-V core @ 160 MHz | Opus encoding consumes 8.5–9.6 ms per 20 ms frame (~48% CPU utilization). Wi-Fi coex or burst traffic causes audio jitter. |
| **Flash on DevKit** | 4 MB on DevKitM-1 module | DevKit cannot use dual-bank A/B OTA partitions (`partitions.csv` requires 8 MB flash). |
| **C Dependencies** | Vendored C `mbedTLS` & `libopus` | Needed because pure-Rust DTLS-SRTP and fixed-point Opus crates with zero heap allocations do not exist for bare-metal RISC-V. |

---

## 5. Migration to Next-Gen SoCs (ESP32-C61 / ESP32-S3)

Migrating the hardware platform to **ESP32-C61** (or **ESP32-S3**) provides substantial architectural advantages for future development:

### Why ESP32-C61 is the Ideal Drop-in Successor
- **Architecture**: Cost-effective single-core RISC-V with Wi-Fi 6 (802.11ax) and BLE 5.
- **Quad SPI PSRAM Support**: The C61 introduces external PSRAM support (missing on C6). Adding 2 MB or 4 MB of PSRAM completely removes the memory squeeze.
- **Stack & Heap Freedom**: The main task stack can be expanded to 64–128 KiB without touching internal SRAM. DTLS handshake structures, network packet buffers, and WebRTC queues can be allocated freely in PSRAM.
- **Pin & Radio Compatibility**: Retains the power profile and 2.4 GHz Wi-Fi 6 performance of the C6 family.

### Alternative: ESP32-S3 (High Performance)
- **Architecture**: Dual-core Xtensa LX7 @ 240 MHz with SIMD/Vector instructions.
- **Core Partitioning**: Core 0 runs Wi-Fi/Matter/TCP networking; Core 1 runs real-time audio DSP, Opus encoding, and full-duplex Acoustic Echo Cancellation (AEC).
- **8 MB Octal PSRAM**: Enables full-duplex PCM ring buffers and multi-frame jitter buffers.

### Path Toward Pure Rust
With 2–8 MB of PSRAM available on ESP32-C61/S3:
1. **Eliminate C libopus**: Standard Rust audio codecs or pure-Rust PCM/G.711 streaming can run without extreme fixed-point static arena tuning.
2. **Eliminate C mbedTLS**: Pure-Rust TLS implementations (such as `rustls` or embedded forks) become viable once the strict 24 KiB static DTLS memory limit is relaxed by PSRAM availability.
3. **Decoupled Architecture Preserved**: All domain logic (`intercom-core`), Matter endpoints (`intercom-adapter-matter`), and HTTP adapters (`intercom-adapter-http`) remain 100% agnostic to the underlying chip target.
