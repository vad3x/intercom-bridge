# Design: Operational Matter-over-TCP Review Fixes

## Context

See [proposal.md](./proposal.md) for motivation and affected crates. Currently on `project-chip/rs-matter#658` (`cadeb52`):
- `rs-matter-stack/src/lib.rs:685` (`run_transport_oper`) misses a `MaybeSend` bound on `F`, breaking `send-sync` builds (`rustcrypto, os` and `windows`).
- `rs-matter-transport/src/network/tcp/embedded.rs:869` runs `std::net::TcpListener::bind` under Miri without `#[cfg_attr(miri, ignore)]`.
- `NetStack::Tcp` is cfg-gated behind `operational-tcp`, breaking downstream `NetStack` impls (`rs-matter-embassy`, `esp-idf-matter`) via Cargo feature unification.
- `TcpServer` and `tcp/std.rs` duplicate TCP framing/state, use `poll_*` traits incompatible with `embassy-net` 0.9 (`smoltcp`), cap frames at 1583 bytes, zero-initialize ~4.9 KB buffers by value inside futures, use 1 peer slot, and close on 2-frame TX queue full or 60s idle without notifying Matter exchanges/sessions.

## Goals / Non-Goals

**Goals:**
- Resolve all 3 CI failures and all 25 review points from `@ivmarkov` in `project-chip/rs-matter#658`.
- Unify `TcpServer` and `tcp/std.rs` into a single `TcpNetwork` (`tcp.rs` + `tcp/frame.rs` + `tcp/std.rs`) over cancel-safe async-fn socket traits.
- Support `embassy-net` 0.9 (`smoltcp`) in a companion `rs-matter-embassy` PR and tune `intercom-matter` for ESP32-C6 (`large-buffers-4096` + `max-im-buffers-4`).

**Non-Goals:**
- Performing OTA updates on hardware (verification uses USB flashing only when user attaches hardware).
- Dropping `const` from `AudioAvStreamHandler::new` or `CameraAvStreamHandler::new`.

## Decisions

### 1. `NetStack::tcp()` Default Method + External Buffer Ownership
- **Choice:** Add `NetTcpStack` next to `NetStack` (`rs-matter-stack/src/net.rs`) and expose `fn tcp(&self) -> Option<impl NetTcpStack + '_> { None::<NoTcp> }` on `NetStack`, removing `operational-tcp` and `stack-tcp` features. `NetTcpStack` holds a reference to caller-owned static TCP buffers; `bind()` returns only the lightweight bound handle.
- **Alternative rejected:** Feature-gating `NetStack::Tcp` (breaks downstream crates via Cargo feature unification) or returning buffers from `bind()` (places multi-KB buffers inside async futures).

### 2. Cancel-Safe Async-fn Socket Traits & Unified `TcpNetwork`
- **Choice:** Replace `poll_*` traits with async-fn traits (`accept()`, `read()`, `write()`, `close()`, `connect()`, `set_keepalive()`), unifying `TcpServer` and `tcp/std.rs` into `TcpNetwork` in `rs-matter-transport/src/network/tcp.rs` (with `async-io` backend in `tcp/std.rs`, split into an initial file-move commit).
- **Rationale:** `embassy-net` 0.9 (`smoltcp`) has no listener object (`TcpSocket::accept()` calls `listen()` on first poll and dropping a socket sends neither FIN nor RST). Calling `accept()` only when a free slot exists and calling explicit `close()` (`abort()` + `flush()`) fits `embassy-net`, `async-io`, and `async-io-mini` cleanly.
- **Safety fixes:** Document `NetworkReceive::wait_available` as cancel-safe (`rs-matter-transport/src/network.rs:314`); remove timed-out connections in `tcp/std.rs` `send_timeout`; read directly into `Vec<u8, N>` without a 256-byte stack bounce buffer or holding blocking mutexes across `.await`; log and retry `accept()` errors.

### 3. Power-of-Two Large Buffers & In-Place `init()`
- **Choice:** Define `large-buffers-4096`..`large-buffers-1048576` (largest wins; `large-buffers` = `large-buffers-65536`; default = UDP size) in one place, deriving transport RX/TX, `IM_BUFFER_SIZE`, and `MAX_TCP_MESSAGE_SIZE`. Parameterize `TcpNetwork<..., const N: usize = MAX_RX_BUF_SIZE, ...>` with `const { assert!(N <= MAX_RX_BUF_SIZE) }`. Use `utils::storage::Vec<u8, N>` + `init_array_from_fn` (`init()`) instead of `[u8; N]` + `Copy` `TxFrame`.
- **Session parameters:** Populate `SUPPORTED_TRANSPORTS` and `MAX_TCP_MESSAGE_SIZE` in `rs-matter-sc::local_session_parameters` (`rs-matter-sc/src/lib.rs:108`) when TCP is enabled, and send `MESSAGE_TOO_LARGE` status report before closing oversized inbound connections.

### 4. Multi-Slot Eviction, Keep-Alive, Backpressure & Session Eviction
- **Choice:** Parameterize `SLOTS` via const generic; evict unauthenticated or longest-idle slots when full. Enable TCP keep-alive (25s interval, 5 probes) instead of 60s idle disconnect. Use bounded wait / byte-budgeted queue with `ErrorCode::ResourceExhausted` (never `ErrorCode::NoSpace`). Report closed TCP peer addresses to `rs-matter-transport` so exchanges close and sessions over TCP are marked/evicted immediately. Dynamically set mDNS `T=4` (server-only) or `T=6` (client+server) after `bind()` succeeds.

### 5. ESP32-C6 SRAM Budget (`intercom-matter`)
- **Choice:** Enable `large-buffers-4096` + `max-im-buffers-4` (4 × 4 KB ≈ 16 KB IM buffers) and place `TcpNetwork` buffers in `StaticCell` via `init()` to preserve `>= 32,768` bytes minimum free heap headroom alongside WebRTC DTLS-SRTP.

## Risks / Trade-offs

- **IM Buffer RAM on ESP32-C6:** `large-buffers-4096` increases every IM buffer to 4 KB → Mitigated by `max-im-buffers-4` and post-call `min_free_heap_bytes >= 32,768` verification on hardware.
- **Transport Session Eviction Coupling:** Propagating TCP close events to `rs-matter-transport` touches exchange/session lifetime → Mitigated by keeping the notification `no_std` and zero-allocation, covered by unit/integration tests.
