# Proposal: Upstream Configurable App Sockets & Stack Access in rs-matter-embassy

## Why

Applications running alongside `rs-matter-embassy` (such as `intercom-firmware`'s HTTP/WebSocket/OTA server on port 8080 and WebRTC ICE/SRTP UDP media engine) need:
1. Additional TCP and UDP socket slots reserved in `embassy_net::StackResources` (`ENET_MIN_SOCKET_SET`) beyond Matter's own sockets (`ENET_EXTRA_TCP_SOCKETS` and `ENET_EXTRA_UDP_SOCKETS`).
2. Access to the initialized `embassy_net::Stack<'static>` handle so independent application tasks can bind sockets and inspect network state.

Upstream `rs-matter-embassy` currently reserves 1 extra UDP socket (`ENET_EXTRA_UDP_SOCKETS = 1`), 0 extra TCP sockets, and does not expose the created `embassy_net::Stack` outside `create_enet_stack`.

## What Changes

- In `rs-matter-embassy/src/enet.rs` (and `Cargo.toml` if feature/const-configurable):
  - Expose configurable extra application socket capacity (at least 2 extra TCP sockets and 2 extra UDP sockets, e.g., via feature flags or `StackResources` sizing) in `ENET_MIN_SOCKET_SET`.
  - Provide an upstream-friendly mechanism to access the initialized `embassy_net::Stack` (either `global_stack()` / `wait_global_stack()` or a `Stack` hook/callback passed into `EmbassyWifi` / `EmbassyEthernet`, migrating `intercom-firmware` if a hook is preferred).
- Explicitly **exclude** `OnceCell<Box<EnetMatterUdpBuffers>>` (`a3be302`'s heap pool), keeping `rs-matter-embassy` `alloc`-free.

## Capabilities

### New Capabilities
- `embassy-app-sockets`: Configurable spare TCP/UDP socket slots in `ENET_MIN_SOCKET_SET` and application access to `embassy_net::Stack`.

### Modified Capabilities
<!-- None -->

## Impact

- **Affected Repositories**:
  - `sysgrok/rs-matter-embassy` (`Cargo.toml`, `src/enet.rs`, optionally `src/eth.rs` / `src/wireless/wifi.rs` if using a hook)
- **API Surface**:
  - `ENET_EXTRA_TCP_SOCKETS` / `ENET_EXTRA_UDP_SOCKETS` (or sizing features) and `Stack` access API.
- **Breaking Changes**: None (default socket counts can remain unchanged when opt-in features are off).
