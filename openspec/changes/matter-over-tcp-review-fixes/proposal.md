# Proposal: Operational Matter-over-TCP Review Fixes

## Why

[project-chip/rs-matter#658](https://github.com/project-chip/rs-matter/pull/658) introduced a bounded embedded Matter-over-TCP runner so controllers like Home Assistant (`matter.js`) can send Large Message commands (such as WebRTC SDP offers). Maintainer review (`@ivmarkov`, 25 points) and CI runs on commit `cadeb52` identified 3 CI failures (`rustcrypto, os`, `windows`, `miri`) and architectural, buffer-sizing, memory-placement, and session-lifecycle gaps that must be resolved before merge, alongside a downstream `rs-matter-embassy` demo PR and ESP32-C6 tuning in `intercom-matter`.

## What Changes

- **CI & Feature Cleanup:** Fix `send-sync` bounds on the operational transport runner, ignore OS-socket/timing tests under Miri with proper waker storage in mocks, remove the `operational-tcp` and `stack-tcp` Cargo features (preserving `const fn new` on AV stream handlers), use `ResourceExhausted` instead of `NoSpace` for queue/slot exhaustion, split the `tcp.rs` -> `tcp/std.rs` move into its own commit, and run operational TCP tests in CI.
- **Large Buffer Sizing & Session Parameters:** Add power-of-two `large-buffers-4096`..`large-buffers-1048576` features (`large-buffers` aliasing `65536`), derive transport, IM, and advertised TCP limits from one definition, advertise `SUPPORTED_TRANSPORTS` and `MAX_TCP_MESSAGE_SIZE` in CASE/PASE handshakes, and emit `MESSAGE_TOO_LARGE` before closing oversized inbound connections.
- **Unified `TcpNetwork`, Async-fn Socket Traits, & Static Placement:** Replace cfg-gated `NetStack::Tcp` with a default `NetStack::tcp()` method returning `NetTcpStack`, replace `poll_*` traits with cancel-safe async-fn socket traits (`accept`, `read`, `write`, `close`, `connect`, keep-alive), unify `TcpServer` and `tcp/std.rs` into `TcpNetwork` with `init()` static buffer placement and direct reads, and fix `send_timeout` connection teardown.
- **Multi-Slot, Keep-Alive, Backpressure, & Session Eviction:** Support multiple peer slots with eviction, replace the 60s idle timeout with TCP keep-alive, apply bounded backpressure on full send queues, propagate TCP close events to close exchanges and evict/mark sessions, and dynamically advertise mDNS `T=4` (server-only) or `T=6` (client+server).
- **Downstream Demo & ESP32-C6 Integration:** Add an `embassy-net` 0.9 (`smoltcp`) backend demo PR in `rs-matter-embassy`, and configure `intercom-matter` with `large-buffers-4096` + `max-im-buffers-4` to fit ESP32-C6's 120 KiB heap / 48 KiB stack budget.

## Capabilities

### New Capabilities
- `matter-over-tcp`: Unified `no_std`-capable operational Matter-over-TCP transport, opt-in network stack discovery, static buffer lifecycle, multi-slot peer management, TCP keep-alive, connection-closed session eviction, and dynamic mDNS `T` capability advertisement.
- `large-buffer-sizing`: Configurable power-of-two transport and Interaction Model buffer sizing, compile-time frame-size validation, CASE/PASE TCP capability advertisement, and oversized-message rejection.

### Modified Capabilities
<!-- None: no existing capability under openspec/specs/ is modified -->

## Impact

- **Affected Repositories:** `project-chip/rs-matter` (`rs-matter-utils`, `rs-matter-transport`, `rs-matter-sc`, `rs-matter-dm-core`, `rs-matter-dm`, `rs-matter-stack`, `rs-matter`), `rs-matter-embassy` (companion demo PR), and `intercom-matter` (static buffer wiring, `large-buffers-4096` + `max-im-buffers-4` profile, USB hardware verification).
- **API & Compatibility:** Downstream `NetStack` implementations compile unchanged without feature flags; `AudioAvStreamHandler::new` and `CameraAvStreamHandler::new` remain `const fn`.
