# Spec Delta: Operational Matter-over-TCP Transport

## Purpose

Defines observable requirements for operational Matter-over-TCP transport discovery, cancel-safe async socket lifecycle, static buffer placement, multi-slot peer management, TCP keep-alive, connection-closed session eviction, and dynamic mDNS TCP capability advertisement.

## ADDED Requirements

### Requirement: Opt-In TCP Stack Discovery Without Feature Gates
The network stack abstraction SHALL expose operational TCP support via an optional discovery method that defaults to disabled, so existing network stack implementations compile unchanged without Cargo feature flags.

#### Scenario: Existing network stack compiles without TCP support
- **WHEN** a platform network stack does not override TCP discovery
- **THEN** it compiles without changes and runs UDP-only operational transport

#### Scenario: Platform network stack enables operational TCP
- **WHEN** a platform network stack provides a TCP stack handle and binding port 5540 succeeds
- **THEN** the Matter stack runs operational TCP concurrently with UDP using caller-provided buffers

### Requirement: Cancel-Safe Async Socket Lifecycle and Timeout Teardown
The TCP transport SHALL operate over cancel-safe async socket operations (accept, read, write, close, connect, and keep-alive) compatible with both OS async I/O and `smoltcp`-based embedded stacks, and SHALL tear down connections on write timeout.

#### Scenario: Embedded stack accepts only with a free slot
- **WHEN** all TCP peer slots are occupied on an embedded stack
- **THEN** the transport does not call accept until a slot becomes free

#### Scenario: Write timeout closes the connection
- **WHEN** writing an outbound frame exceeds the configured send timeout
- **THEN** the transport closes and removes that peer connection so no partial frame remains on the stream

### Requirement: In-Place Static Buffer Placement and Non-Blocking I/O
TCP transport buffers SHALL support in-place initialization in static storage so large frame arrays are neither zeroed on the stack nor stored inside async task futures, and async socket I/O SHALL NOT hold blocking critical-section mutexes or bounce through intermediate stack buffers.

#### Scenario: Buffers survive network restarts in static storage
- **WHEN** the network interface restarts and rebinds the TCP listener
- **THEN** caller-owned static TCP buffers are reused without re-allocating on the stack or inside futures

### Requirement: Multi-Slot Peer Concurrency and Error Recovery
The operational TCP transport SHALL support a configurable number of concurrent peer slots, SHALL evict unauthenticated or longest-idle connections when full, and SHALL recover from transient accept errors without stopping UDP transport.

#### Scenario: Multiple controllers connect concurrently
- **WHEN** two controllers (such as Home Assistant and Apple Home) maintain active TCP connections
- **THEN** both controllers remain connected in separate slots without disrupting each other

#### Scenario: Transient accept error does not abort transport
- **WHEN** accepting an inbound TCP connection returns a socket error
- **THEN** the transport logs the error and continues serving active TCP and UDP traffic

### Requirement: TCP Keep-Alive, Bounded Backpressure, and Resource Error Reporting
The TCP transport SHALL use TCP keep-alive rather than a short application idle timeout, SHALL apply bounded backpressure instead of immediately closing on a full send queue, and SHALL report queue or slot exhaustion as resource exhaustion rather than TLV space exhaustion.

#### Scenario: Quiet signaling session stays open via keep-alive
- **WHEN** a controller completes an SDP exchange over TCP and sends no Matter traffic for several minutes while remaining reachable
- **THEN** TCP keep-alive preserves the connection without an application idle disconnect

#### Scenario: Full send queue reports resource exhaustion on timeout
- **WHEN** the outbound queue cannot accept a frame within the bounded wait
- **THEN** the transport reports a resource-exhaustion error rather than a TLV write-space error

### Requirement: Connection-Closed Session and Exchange Eviction
When a TCP peer connection closes, times out, or fails, the TCP transport SHALL notify the Matter transport layer immediately so affected exchanges are closed and sessions bound to that peer are evicted or marked for reconnection.

#### Scenario: Peer disconnect cleans up sessions immediately
- **WHEN** a TCP peer closes or resets its connection
- **THEN** the Matter transport closes active exchanges and evicts or marks sessions bound to that peer address

### Requirement: Dynamic mDNS TCP Capability Advertisement
The operational mDNS advertisement SHALL set the `T` TXT capability key dynamically only after the TCP listener binds, advertising server-only capability (`T=4`) when only inbound connections are supported or client-and-server capability (`T=6`) when both roles are active.

#### Scenario: Server-only node advertises T=4
- **WHEN** the node binds an operational TCP server listener without outbound TCP client support
- **THEN** `_matter._tcp.local` advertises `T=4`

#### Scenario: Client-and-server node advertises T=6
- **WHEN** both operational TCP server and client roles are enabled and bound
- **THEN** `_matter._tcp.local` advertises `T=6`
