# Spec Delta: Embassy Network Sizing, On-Demand Buffers & Wi-Fi 6 Support

## Purpose

Optimizes Embassy async runtime networking for rs-matter by enabling on-demand TCP socket buffers, configurable socket allocation, and robust Wi-Fi 6 association tuning.

## ADDED Requirements

### Requirement: On-Demand TCP Buffer Allocation
The Embassy network runner SHALL allocate TCP RX/TX socket buffers dynamically when a TCP connection is initiated, rather than reserving them statically at boot.

#### Scenario: TCP buffers unallocated during idle boot
- **WHEN** the device boots and runs with no active Matter TCP or HTTP connections
- **THEN** no static memory is reserved for inactive TCP socket buffers
- **AND** available system SRAM remains available for other tasks

#### Scenario: Buffers released on TCP connection teardown
- **WHEN** an active Matter-over-TCP session terminates
- **THEN** the allocated TCP RX and TX buffers are deallocated and returned to the system pool

### Requirement: Configurable Extra TCP Socket Capacity
The Embassy integration crate SHALL provide compile-time feature flags to reserve additional TCP sockets for concurrent background services such as REST APIs, WebSockets, or OTA downloads.

#### Scenario: Concurrent REST and WebSocket operations
- **WHEN** the crate is compiled with enet-extra-sockets-2 enabled
- **THEN** embassy-net allocates socket pool entries allowing two concurrent external TCP streams alongside Matter UDP and TCP sockets

### Requirement: Wi-Fi 6 (802.11ax) Protocol Negotiation
The wireless driver integration SHALL support 802.11ax protocol negotiation flags when running on Wi-Fi 6 capable SoCs.

#### Scenario: Associating to 802.11ax access point
- **WHEN** the device scans and associates to a Wi-Fi 6 compatible access point
- **THEN** the Wi-Fi driver includes 802.11ax capability parameters during the association handshake

### Requirement: Fast AP Association and All-Channel Scanning
The Wi-Fi provisioning and re-connection logic SHALL use fast scan methods and full channel dwell time to minimize reconnection latency after a temporary network drop.

#### Scenario: Immediate reconnect after AP beacon loss
- **WHEN** the device loses association to the configured access point
- **THEN** the connection manager uses fast scanning on the last known channel before triggering an all-channel scan
- **AND** re-establishes network connectivity within the fast scan window
