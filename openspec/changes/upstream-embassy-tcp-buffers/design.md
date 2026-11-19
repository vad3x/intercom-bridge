# Design: Upstream On-Demand TCP Buffers in rs-matter-embassy

## Context

Matter-over-TCP requires TCP RX/TX buffers (typically 2048 bytes each plus socket state). Pre-allocating these statically reduces the main thread's stack headroom in `esp-hal`.

## Decisions

### 1. Dynamic Allocation via OnceCell/Heap
- **Decision**: In `EmbassyNetContext`, store `tcp_buffers: OnceCell<Box<EnetMatterTcpBuffers>>` rather than an inline struct.
- **Rationale**: Keeps `.bss` clean at boot; leaves 16–24 KiB free for Opus Silk recursive pitch search.

### 2. Dependency on PR 2's TCP Network Trait
- **Decision**: Develop against `rs-matter`'s TCP network trait (`NetStack` or `TcpAccept`), deferring upstream submission to `sysgrok/rs-matter-embassy` until PR 2 stabilizes in `project-chip/rs-matter`.
- **Rationale**: Ensures upstream CI on `rs-matter-embassy` succeeds when building against `project-chip/rs-matter`.

## Migration Plan

1. Await or coordinate with `upstream-matter-over-tcp` PR 2 trait definition.
2. Cut clean branch `feat/embassy-tcp-buffers` in `sysgrok/rs-matter-embassy`.
3. Implement dynamic buffer management and unit tests.
4. Submit PR to `sysgrok/rs-matter-embassy`.
5. Archive change with `openspec archive upstream-embassy-tcp-buffers`.
