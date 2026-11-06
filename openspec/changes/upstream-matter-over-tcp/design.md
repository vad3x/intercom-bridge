# Design: Embedded Matter-over-TCP Integration & Operational Runner in rs-matter

## Context

Matter operational communication traditionally runs over UDP with Matter Reliable Protocol (MRP). For large data exchanges or environments with high packet loss, Matter 1.3+ specifies operational transport over TCP on port 5540 with 4-byte little-endian framing.

## Decisions

### 1. Matter-over-TCP Framing and Embedded Stack Integration
- **Decision**: Use Matter's 4-byte little-endian length prefix. Preserve the existing host `TcpNetwork` framing in `rs-matter-transport`; implement or reuse the framing logic needed by the `no_std` embedded listener in `rs-matter-stack`.
- **Rationale**: Strict compliance with Matter specification section 4.3 (TCP Transport). Reuses existing framing structures while keeping embedded code `no_std`.

### 2. Non-Blocking Cooperative Polling in `rs-matter-stack`
- **Decision**: In `rs-matter-stack`, run the TCP listener and stream handling cooperatively with UDP MRP using non-blocking async futures (`select` / `join`).
- **Rationale**: Embedded targets run on single-threaded executors (e.g. Embassy). TCP reads/writes must yield cooperatively so UDP MRP keep-alives and retransmissions are not starved.

### 3. Operational mDNS TCP Capability
- **Decision**: Set the operational device details' `tcp_supported` flag when the embedded TCP server is enabled, advertising `T=6` (client + server) in mDNS TXT.
- **Rationale**: Reuses the upstream mDNS encoder, advertising TCP capability only when configured.

### 4. Single-Connection Concurrency Control
- **Decision**: Restrict embedded targets to 1 active operational TCP peer; reject or defer subsequent attempts cleanly without disrupting the active session.
- **Rationale**: Prevents socket/memory exhaustion on microcontrollers while supporting standard Matter controller connections.

## Migration Plan

1. Cut clean feature branch `feat/matter-over-tcp` from `upstream/main` in `project-chip/rs-matter`.
2. Implement TCP framing, listener, and operational runner in `rs-matter-stack`.
3. Add tests, run clippy and fmt.
4. Submit PR to `project-chip/rs-matter`.
5. Archive change with `openspec archive upstream-matter-over-tcp`.
