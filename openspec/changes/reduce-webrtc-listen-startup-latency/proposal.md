# Proposal

## Why

Starting a WebRTC Listen session from Matter / Home Assistant currently takes ~5 seconds before audio begins streaming, which degrades real-time intercom responsiveness. Profiling shows the delay is dominated by redundant synchronous software ECDSA P-256 certificate generation (executed twice per session while blocking the Core 0 Embassy executor), single-datagram-per-20ms-tick DTLS BIO buffering, `-Os`-compiled MbedTLS ECC operations, and a 1.0 s silence pause in the bench test recording.

## What Changes

- **Cached DTLS Identity at Boot**: Generate and cache the ECDSA P-256 keypair, self-signed X.509 certificate DER, and SHA-256 fingerprint once at startup (or per-device lifetime) instead of regenerating them synchronously on every `ProvideOffer` and `WebRtcMediaEngine::run_session` call.
- **Multi-Datagram DTLS BIO & Immediate Flush**: Drain all pending outbound DTLS handshake datagrams in a loop per step until `WANT_READ`/`WANT_WRITE`, eliminating the 1-datagram-per-20ms-tick bottleneck during multi-datagram DTLS flights.
- **MbedTLS Optimization Level**: Compile `dtls_intercom.c` and vendored MbedTLS ECC/DTLS sources with `-O2`/`-O3` instead of `-Os` to accelerate software P-256 ECDHE/ECDSA handshake operations.
- **Bench Test Audio Onset**: Reduce or eliminate the 1.0 s `repeat_silence_samples` pause on the bench test PCM loop so synthetic test audio begins immediately when DTLS/SRTP completes.

## Capabilities

### New Capabilities
- `webrtc-media-startup`: Fast WebRTC Listen session startup covering cached DTLS ECDSA identity provisioning, multi-datagram DTLS handshake flushing, and immediate media onset.

### Modified Capabilities
<!-- No existing specs modified. -->

## Impact

- `crates/intercom-adapter-webrtc/csrc/dtls_intercom.c` and `dtls_intercom.h`: Support initializing DTLS sessions from a pre-generated/cached keypair and certificate DER (or caching the identity across sessions) without re-running `mbedtls_ecp_gen_key` + `mbedtls_x509write_crt_der` per call.
- `crates/intercom-adapter-webrtc/src/dtls.rs`, `signaling.rs`, `session.rs`: Reuse cached DTLS identity/fingerprint in `ProvideOffer` and `run_session`; drain all outbound DTLS datagrams per handshake step.
- `crates/intercom-adapter-webrtc/build.rs`: Switch MbedTLS C compilation from `-Os` to `-O2` (or `-O3` where flash budget permits).
- `crates/intercom-firmware/src/main.rs` (and/or `platform`): Provision cached DTLS identity at boot and adjust bench `repeat_silence_samples` if configured for immediate onset.
