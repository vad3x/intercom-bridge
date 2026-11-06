# Spec Delta: Intercom Adapter Wi-Fi 6, Upstream Sockets & On-Demand TCP Buffers

## Purpose

Provides hardware-optimized wireless performance, socket headroom for application HTTP/WebSocket servers, and dynamic SRAM management for Matter-over-TCP on ESP32-C6 in `intercom-adapter-matter`.

## ADDED Requirements

### Requirement: Wi-Fi 6 (802.11ax) Protocol Negotiation
The `IntercomWifiDriver` SHALL pass 802.11ax protocol capability flags during Wi-Fi association on ESP32-C6.

#### Scenario: Associating to 802.11ax access point
- **WHEN** the device connects to an 802.11ax access point
- **THEN** the Wi-Fi controller negotiates with `Protocol::AX` enabled

### Requirement: Fast AP Association and RSSI Sorting
The wireless driver SHALL use `ScanMethod::Fast` for immediate reconnect on the last known channel before all-channel scan, and SHALL sort scan results descending by RSSI while deduplicating identical BSSIDs.

#### Scenario: Fast reconnect after network drop
- **WHEN** the device loses association to the AP
- **THEN** it immediately scans the last known channel using fast scan method

### Requirement: Upstream Embassy Extra Socket Capacity
The firmware build SHALL consume upstream `rs-matter-embassy` with `features = ["enet-extra-sockets-2"]` to reserve socket headroom for concurrent REST and WebSocket servers.

#### Scenario: Concurrent REST and WebSocket operations
- **WHEN** the device runs with the Solid.js dashboard and active WebSocket connections
- **THEN** external TCP connections succeed without starving Matter protocol sockets

### Requirement: On-Demand Matter TCP Buffer Lifecycle
The adapter SHALL allocate TCP socket RX and TX buffers dynamically on incoming TCP connections and release them upon disconnection or idle timeout.

#### Scenario: Buffer release on disconnect
- **WHEN** a Matter-over-TCP session terminates
- **THEN** the 16–24 KiB of TCP socket buffers are freed back to the system heap
