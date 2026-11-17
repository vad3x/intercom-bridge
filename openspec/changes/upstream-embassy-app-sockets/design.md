# Design: Upstream Configurable App Sockets & Stack Access in rs-matter-embassy

## Context

In `review_matter_tcp_rust` (`fd8010a`, `49c50f8`, `a3be302`), `intercom-matter` added:
- `ENET_EXTRA_TCP_SOCKETS = 2` (for concurrent HTTP REST + WebSocket/OTA)
- `ENET_EXTRA_UDP_SOCKETS = 2` (for WebRTC + app UDP)
- `global_stack()` / `wait_global_stack()` backed by a `CriticalSection` `GLOBAL_STACK` + `Signal`

Upstream `sysgrok/rs-matter-embassy` prefers zero `alloc` and minimal global mutable state, so this change separates the socket sizing + `Stack` access from `a3be302`'s `OnceCell<Box<...>>` heap pool.

## Decisions

### 1. Configurable Extra TCP and UDP Socket Counts
- **Decision**: Support reserving additional app TCP and UDP sockets in `ENET_MIN_SOCKET_SET` (e.g. via Cargo features like `extra-tcp-sockets-N` / `extra-udp-sockets-N` matching `max-subscriptions-N` in `Cargo.toml`, or `ENET_EXTRA_TCP_SOCKETS` / `ENET_EXTRA_UDP_SOCKETS` constants).
- **Rationale**: Keeps default builds minimal while allowing `intercom-firmware` to reserve 2 extra TCP + 2 extra UDP sockets without a fork.

### 2. Application `Stack` Access
- **Decision**: Propose `global_stack()` / `wait_global_stack()` or a callback/channel hook on `create_enet_stack` / `EmbassyWifi` / `EmbassyEthernet` (aligning with `sysgrok` review preference before finalizing).
- **Rationale**: Independent `embassy-executor` tasks (`http_server`, `webrtc`, `hardware_info`) need `embassy_net::Stack<'static>` once Wi-Fi/Ethernet comes up.

### 3. No `OnceCell<Box<...>>` Heap Pool
- **Decision**: Omit the `extern crate alloc` and `OnceCell<Box<EnetMatterUdpBuffers>>` part of `a3be302`; tune `HEAP_SIZE` in `intercom-firmware` instead.
- **Rationale**: Keeps `rs-matter-embassy` `alloc`-free and avoids shifting bytes between `.bss` and heap without real peak-RAM savings.

## Migration Plan

1. Cut clean feature branch `feat/app-sockets-and-stack-access` from `upstream/master` in `sysgrok/rs-matter-embassy`.
2. Implement configurable extra TCP/UDP socket sizing and `Stack` access API.
3. Verify `cargo fmt`, `cargo clippy`, and `cargo test` across host and `riscv32imac-unknown-none-elf`.
4. Submit PR to `sysgrok/rs-matter-embassy` (and update `intercom-firmware` in `downstream-intercom-integration` if the `Stack` access API changes from `global_stack()`).
5. Archive change with `openspec archive upstream-embassy-app-sockets`.
