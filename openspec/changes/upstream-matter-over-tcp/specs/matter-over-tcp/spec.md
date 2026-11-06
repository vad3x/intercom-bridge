# Spec Delta: Matter-over-TCP Operational Transport & Framing

## Purpose

Provides reliable Matter-over-TCP operational transport (port 5540) with Matter-compatible 4-byte little-endian framing, mDNS TCP service advertisement, and cooperative async polling alongside UDP in rs-matter.

## ADDED Requirements

### Requirement: Matter TCP Transport Framing
The embedded TCP transport SHALL encode and decode Matter operational messages using a 4-byte little-endian unsigned length prefix prepended to each Matter message. Existing host TCP framing in `rs-matter-transport` SHALL remain compatible with this format.

#### Scenario: Inbound framed packet reception
- **WHEN** an inbound TCP stream delivers bytes containing a 4-byte little-endian length header followed by the complete payload
- **THEN** the transport engine parses the length header, extracts the exact framed message, and dispatches it to the Matter session manager

#### Scenario: Outbound framed packet transmission
- **WHEN** the session manager sends a message over an established TCP transport
- **THEN** the transport engine prefixes the message with its 4-byte little-endian length and writes the framed buffer to the TCP socket

### Requirement: Operational Discovery mDNS TCP Flag Advertisement
The operational mDNS TXT record SHALL include `T=6` when the device supports both TCP client and TCP server operation. The existing transport encoder SHALL be reused, and the stack SHALL set TCP support only when its operational TCP server is enabled.

#### Scenario: Operational discovery includes TCP transport flag
- **WHEN** the device advertises its operational identity via mDNS service _matter._tcp.local
- **THEN** the TXT record includes key "T" with value "6" indicating TCP client (2) and TCP server (4) support

### Requirement: Non-Blocking Cooperative Async Network Execution
The Matter stack network runner SHALL run the TCP listener and connection handlers concurrently with UDP MRP without blocking or starving UDP packets.

#### Scenario: Concurrent UDP and TCP message processing
- **WHEN** the device receives UDP MRP traffic and active TCP streaming data simultaneously
- **THEN** the async network runner yields cooperatively across both transports
- **AND** UDP exchanges proceed without missing retransmission timeouts or keep-alives

### Requirement: Inbound TCP Connection Lifecycle and Idle Timeout
The TCP transport listener SHALL accept inbound client connections on port 5540, maintain session state, and cleanly close idle connections after a configurable inactivity threshold.

#### Scenario: Idle TCP connection termination
- **WHEN** an established TCP peer sends no packets and responds to no keep-alives for the configured idle duration
- **THEN** the TCP listener closes the socket connection and releases associated buffers

### Requirement: Single-Connection Concurrency Control
On resource-constrained embedded targets, the TCP listener SHALL support at least one active Matter-over-TCP peer and reject or defer subsequent connection attempts cleanly without crashing.

#### Scenario: Second TCP client connection while active
- **WHEN** a second Matter controller attempts a TCP handshake while an existing TCP session is active
- **THEN** the transport engine closes or refuses the new connection without disrupting the active operational session

### Requirement: Optional Wi-Fi-Only Startup Runner
The stack SHALL provide a Wi-Fi-only runner configuration that bypasses BLE GATT initialization when network credentials are already provisioned.

#### Scenario: Fast boot without BLE startup
- **WHEN** a provisioned device starts using the WifiOnly runner
- **THEN** the Matter stack immediately initiates Wi-Fi connectivity and operational mDNS discovery without initializing or powering on the BLE controller
