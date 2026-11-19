# Tasks

## 1. Cached DTLS Identity in `dtls_intercom.c` and `dtls.rs`

- [x] 1.1 **[RED]** Write failing unit tests in `crates/intercom-adapter-webrtc/src/dtls.rs` asserting that `DtlsIdentity::generate([0x42; 32], &mut resources)` exports a non-empty certificate DER, private key DER, and 32-byte SHA-256 fingerprint, and that two `DtlsSrtpEngine::new_with_identity` instances initialized from that same `DtlsIdentity` complete a full client/server DTLS-SRTP handshake with identical local fingerprints and matching exported SRTP keys; verify failure with `mise exec -- cargo test -p intercom-adapter-webrtc --lib --target x86_64-apple-darwin dtls::tests::cached_identity`.
- [x] 1.2 **[GREEN]** Implement `intercom_dtls_generate_identity` and `intercom_dtls_init_with_identity` in `crates/intercom-adapter-webrtc/csrc/dtls_intercom.c` and `csrc/dtls_intercom.h`, expose `DtlsIdentity` and `DtlsSrtpEngine::new_with_identity` in `crates/intercom-adapter-webrtc/src/dtls.rs`, and verify the tests pass with `mise exec -- cargo test -p intercom-adapter-webrtc --lib --target x86_64-apple-darwin dtls::tests::cached_identity`.

## 2. Reuse Cached DTLS Identity in Signaling and Media Session

- [x] 2.1 **[RED]** Write failing tests in `crates/intercom-adapter-webrtc/src/signaling.rs` and `crates/intercom-adapter-webrtc/src/session.rs` asserting that `WebRtcSignalingCoordinator` and `WebRtcMediaEngineConfig` accept a cached `DtlsIdentity` (returning its precomputed `fingerprint_sha256` during `ProvideOffer` without touching `DtlsSrtpResources` or invoking keygen) and pass it through to `WebRtcMediaEngine::run_session`; verify failure with `mise exec -- cargo test -p intercom-adapter-webrtc --lib --target x86_64-apple-darwin signaling::tests::`.
- [x] 2.2 **[GREEN]** Wire cached `DtlsIdentity` through `signaling.rs`, `session.rs`, and firmware boot initialization (`crates/intercom-firmware/src/main.rs`), and verify all host tests pass with `mise exec -- cargo test -p intercom-adapter-webrtc --lib --target x86_64-apple-darwin`.

## 3. Multi-Datagram DTLS BIO Ring & Drain Loop

- [x] 3.1 **[RED]** Write failing unit tests in `crates/intercom-adapter-webrtc/src/dtls.rs` and `crates/intercom-adapter-webrtc/src/session.rs` asserting that a multi-datagram DTLS server flight queues all datagrams in `intercom_bio_send` without returning `WANT_WRITE` on the second datagram, and that `drain_dtls_outbound` / `step_dtls_and_pop` drains every queued datagram in one step; verify failure with `mise exec -- cargo test -p intercom-adapter-webrtc --lib --target x86_64-apple-darwin multi_datagram`.
- [x] 3.2 **[GREEN]** Replace the single `output_pending` buffer in `crates/intercom-adapter-webrtc/csrc/dtls_intercom.c` with a bounded outbound datagram FIFO (updating `STATE_STORAGE_WORDS` in `dtls.rs` if needed) and loop outbound draining per handshake step in `crates/intercom-adapter-webrtc/src/session.rs`; verify tests pass with `mise exec -- cargo test -p intercom-adapter-webrtc --lib --target x86_64-apple-darwin`.

## 4. MbedTLS Compiler Optimization & Immediate Bench Audio Onset

- [x] 4.1 **[RED]** Add a test in `crates/intercom-media/src/lib.rs` (or `crates/intercom-adapter-audio`) verifying that configuring bench test speech with `repeat_silence_samples = 0` emits non-zero speech PCM samples on the very first read frame without a leading silence gap; verify failure/baseline with `mise exec -- cargo test -p intercom-media --lib --target x86_64-apple-darwin`.
- [x] 4.2 **[GREEN]** Update `crates/intercom-adapter-webrtc/build.rs` to compile `mbedtls_dtls_srtp` with `-O2` instead of `-Os`, set bench `repeat_silence_samples` to `0` where immediate onset is desired, and verify host tests plus firmware builds (`RUSTC_BOOTSTRAP=1 mise exec -- cargo build -p intercom-firmware --release`, `mise exec -- cargo build-devkit`, `mise exec -- cargo build-s3`) succeed cleanly within partition limits.

## 5. Startup Latency Benchmark Test & Telemetry

- [x] 5.1 **[RED]** Write failing tests in `crates/intercom-adapter-webrtc/src/session.rs` (`MediaBenchmarkStats`) asserting that startup latency fields (`dtls_init_us`, `dtls_handshake_ms`, `first_srtp_packet_ms`) are recorded and formatted, and add a host benchmark test in `crates/intercom-adapter-webrtc/src/dtls.rs` (`dtls::tests::benchmark_cached_vs_uncached_startup`) asserting that cached `DtlsSrtpEngine::new_with_identity` initialization is faster than `DtlsSrtpEngine::new` and multi-datagram handshake completes in fewer poll steps; verify failure with `mise exec -- cargo test -p intercom-adapter-webrtc --lib --target x86_64-apple-darwin benchmark_`.
- [x] 5.2 **[GREEN]** Extend `MediaBenchmarkStats` and `WebRtcMediaEngine::run_session` in `crates/intercom-adapter-webrtc/src/session.rs` to capture and log `dtls_init_us`, `dtls_handshake_ms`, and `first_srtp_packet_ms`, implement the host comparative benchmark test in `dtls.rs`, and verify all benchmark tests pass with `mise exec -- cargo test -p intercom-adapter-webrtc --lib --target x86_64-apple-darwin benchmark_ -- --nocapture`.

## 6. Verification

- [x] 6.1 Run full host test suite (`mise exec -- cargo test -p intercom-core -p intercom-media -p intercom-adapter-http -p intercom-adapter-audio -p intercom-adapter-webrtc -p intercom-firmware --lib --target x86_64-apple-darwin`), verify `reduce-webrtc-listen-startup-latency` passes `openspec validate reduce-webrtc-listen-startup-latency --strict`, and measure end-to-end Listen startup latency on hardware (<1 s target from `ProvideOffer` to `WebRTC: first outbound SRTP audio packet sent`) using the `WebRTC media benchmark` log output.
