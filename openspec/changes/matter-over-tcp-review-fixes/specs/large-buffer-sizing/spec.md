# Spec Delta: Large Buffer Sizing and Matter TCP Session Parameters

## Purpose

Defines power-of-two transport and Interaction Model buffer sizing features, compile-time frame-size validation, Matter CASE/PASE TCP capability advertisement, and oversized-frame error reporting.

## ADDED Requirements

### Requirement: Power-of-Two Large Buffer Sizing Features
The Matter crates SHALL expose power-of-two large-buffer features from 4096 bytes (`large-buffers-4096`) through 1048576 bytes (`large-buffers-1048576`) where the largest enabled feature wins, `large-buffers` aliases 65536 bytes, and no feature retains UDP-sized buffers.

#### Scenario: Constrained embedded build selects 4096-byte buffers
- **WHEN** a firmware crate enables the 4096-byte buffer feature alongside reduced Interaction Model buffer count
- **THEN** transport buffers, Interaction Model buffers, and default max TCP message size all derive from 4096 bytes

#### Scenario: Default large-buffers feature selects 65536 bytes
- **WHEN** a crate enables `large-buffers` without a larger size feature
- **THEN** transport and Interaction Model buffers use 65536 bytes

### Requirement: Compile-Time Frame Size Consistency
The TCP transport SHALL parameterize its maximum frame size with a default matching the configured transport receive buffer size and SHALL enforce at compile time that the TCP frame limit does not exceed the transport receive buffer.

#### Scenario: Oversized TCP frame constant fails compilation
- **WHEN** a caller instantiates the TCP transport with a maximum frame size larger than the transport receive buffer
- **THEN** compilation fails with a compile-time assertion error

### Requirement: CASE and PASE TCP Parameter Advertisement
When operational TCP is enabled, CASE and PASE session establishment SHALL advertise `SUPPORTED_TRANSPORTS` and `MAX_TCP_MESSAGE_SIZE` matching the node's configured TCP frame limit.

#### Scenario: Controller reads TCP capability during CASE
- **WHEN** a controller establishes a CASE session with a TCP-capable node
- **THEN** the node's session parameters include `SUPPORTED_TRANSPORTS` and `MAX_TCP_MESSAGE_SIZE`

### Requirement: Oversized Inbound Frame Rejection
When an inbound TCP frame header declares a payload larger than the node's maximum frame size, the node SHALL transmit a `MESSAGE_TOO_LARGE` status report before closing the connection.

#### Scenario: Peer sends oversized TCP frame
- **WHEN** an inbound TCP connection sends a 4-byte length header exceeding the node's maximum frame size
- **THEN** the node sends a `MESSAGE_TOO_LARGE` status report and closes that connection without stalling other peers or UDP
