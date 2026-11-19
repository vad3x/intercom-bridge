# Design

## Context

See `proposal.md` for motivation and latency breakdown. Currently:
- `crates/intercom-adapter-webrtc/src/signaling.rs` (`local_fingerprint`) constructs a full `DtlsSrtpEngine::new` during `ProvideOffer` solely to read `local_cert_sha256()`, then drops it.
- `crates/intercom-adapter-webrtc/src/session.rs` (`WebRtcMediaEngine::run_session`) constructs `DtlsSrtpEngine::new` a second time, repeating `mbedtls_ecp_gen_key(MBEDTLS_ECP_DP_SECP256R1)` and `mbedtls_x509write_crt_der` in `csrc/dtls_intercom.c` (`intercom_make_self_signed_certificate`) on Core 0.
- `csrc/dtls_intercom.c` (`intercom_bio_send`) holds only a single `output_pending` buffer (`output[1500]`), and `session.rs` (`step_dtls_and_pop`) pops at most one datagram per `MEDIA_POLL_MS` (20 ms) loop iteration.
- `crates/intercom-adapter-webrtc/build.rs` compiles `dtls_intercom.c` and vendored MbedTLS with `-Os`.
- Bench builds loop `devkit_test_speech_short.pcm` with `repeat_silence_samples = 16_000` (1.0 s) while Opus DTX is enabled.

## Goals / Non-Goals

**Goals:**
- Reduce Matter/WebRTC Listen startup latency from ~5 s to sub-second (<1 s) on ESP32-C6 and ESP32-S3.
- Eliminate all per-call synchronous ECDSA P-256 key generation and X.509 certificate signing.
- Flush multi-datagram DTLS flights in a single `session.rs` handshake step.
- Accelerate MbedTLS software P-256 ECC without exceeding C6/S3 flash or RAM budgets.

**Non-Goals:**
- Switching from DTLS 1.2 to DTLS 1.3 or replacing MbedTLS.
- Removing DTLS cookie verification (`HelloVerifyRequest`) unless needed after measuring the primary wins (kept as a follow-up option if RTT remains high).
- Implementing hardware ECC peripheral acceleration in MbedTLS `ALT` hooks (out of scope for this change; compiler optimization + boot-time key caching address the bottleneck first).

## Decisions

### 1. Boot-Time Cached `DtlsIdentity` (`DER cert + ECDSA private key DER + SHA-256 fingerprint`)
- **Decision**: Introduce a lightweight `DtlsIdentity` struct (cached in static/heap storage once at firmware boot or lazily on first use via `DtlsSrtpResources`) holding the serialized self-signed X.509 certificate DER, ECDSA P-256 private key DER (e.g., via `mbedtls_pk_write_key_der`), and 32-byte SHA-256 fingerprint.
  - `signaling.rs` reads `identity.fingerprint_sha256` directly in O(1) time without touching MbedTLS or `DtlsSrtpResources`.
  - `DtlsSrtpEngine::new_with_identity(...)` parses the pre-generated cert DER (`mbedtls_x509_crt_parse_der`) and private key DER (`mbedtls_pk_parse_key`) into `state->local_cert` / `state->local_key`—which takes milliseconds instead of ~1 s+ of keygen + ECDSA signing.
- **Alternative Considered**: Persisting a static identity in NVS/flash across reboots. Rejected for now because boot-time generation once per power cycle (~1 s during boot, off the Listen critical path) avoids flash wear and key-storage migration while achieving the same zero-keygen latency per Listen call.

### 2. Multi-Datagram Outbound BIO Ring + Drain Loop
- **Decision**: Replace the single `output_pending` slot in `intercom_dtls_state` (`dtls_intercom.c`) with a small fixed FIFO of datagram slots (e.g., 4–6 slots × 1500 bytes, or keep `intercom_bio_send` returning `WANT_WRITE` only when the ring is full), and update `session.rs` to loop `step_dtls_and_pop` / `pop_outbound_datagram` sending all queued datagrams via `socket.send_to` until empty and `dtls.step` stops returning `WANT_WRITE` or producing packets.
- **Alternative Considered**: Increasing `MEDIA_POLL_MS` frequency during handshake only without changing the 1-datagram BIO buffer. Rejected because a 2-to-4-datagram DTLS flight would still require multiple async yield cycles and timer ticks instead of bursting cleanly over UDP.

### 3. MbedTLS `-O2` Compilation in `build.rs`
- **Decision**: Change `.flag_if_supported("-Os")` to `.flag_if_supported("-O2")` for `mbedtls_dtls_srtp` in `crates/intercom-adapter-webrtc/build.rs`, verifying ESP32-C6 (4 MB DevKit & 8 MB) and ESP32-S3 binary sizes stay comfortably within partition limits.
- **Alternative Considered**: `-O3` globally on MbedTLS. `-O2` gives most of the bignum/ECC loop speedups without excessive code-size bloat on 4 MB C6 DevKit; can benchmark `-O3` on ECC-only files if needed.

### 4. Bench Test PCM Onset
- **Decision**: Configure bench `EmbeddedTestSample` / `devkit_test_speech_short.pcm` repeat pause (`repeat_silence_samples`) to `0` (or a short tail after speech rather than before speech, or `0` ms) so first audio starts immediately on SRTP ready.

### 5. Startup Latency Benchmark Test & Telemetry
- **Decision**: Extend `MediaBenchmarkStats` in `crates/intercom-adapter-webrtc/src/session.rs` to record `dtls_init_us`, `dtls_handshake_ms`, and `first_srtp_packet_ms` (measured from `run_session` entry), logging them in the existing `WebRTC media benchmark ...` summary line. On host, add a dedicated benchmark test in `dtls.rs` / `session.rs` comparing uncached `DtlsSrtpEngine::new` vs cached `DtlsSrtpEngine::new_with_identity` + multi-datagram handshake step count and elapsed microseconds.
- **Alternative Considered**: Adding a separate `criterion` dev-dependency. Avoided to keep `no_std` workspace dependencies minimal and runnable directly via `cargo test`.

## Risks / Trade-offs

- **RAM overhead for cached `DtlsIdentity` and multi-datagram BIO**:
  - `DtlsIdentity` DER buffers are tiny (~450 B cert + ~130 B key + 32 B fingerprint ≈ 620 B).
  - A 4-slot 1500 B BIO ring adds ~4.5 KiB inside `DtlsSrtpState` (`STATE_STORAGE_WORDS` in `dtls.rs`).
  - *Mitigation*: Verify `STATE_STORAGE_WORDS` static assertion / unit test and check C6 free heap headroom (`>= 32,768` bytes) and S3 PSRAM/internal heap after Listen calls.
- **Flash size increase from `-O2` on MbedTLS**:
  - *Mitigation*: Build `esp32c6` (`devkit` 4 MB and 8 MB) and `esp32s3` release images to confirm partition fit before finalizing.
- **DTLS `HelloVerifyRequest` RTT**:
  - Keeping `mbedtls_ssl_conf_dtls_cookies` preserves standard DTLS anti-amplification on Matter/ICE-verified peers; if measured handshake latency after the above changes still exceeds target, cookie bypass for already-ICE-verified peers can be evaluated separately.
