# Design

## Context

- `intercom-adapter-http/src/server.rs` currently uses `picoserve::response::ws::WebSocketUpgrade` for `/api/ws/audio` and runs **2** `accept_worker` tasks (`join3(accept_worker(0), accept_worker(1), proxy.run(...))`).
- Each `accept_worker` allocates `TCP_RX_BUFFER_SIZE + TCP_TX_BUFFER_SIZE + HTTP_BUFFER_SIZE` on its async future.
- A persistent `/api/ws/status` connection holds one `accept_worker` for its entire lifetime. If `/api/ws/audio` is also open during Push-to-Talk, 2 workers would leave 0 workers for REST (`/api/action/unlock`, `/api/logs`, OTA).
- Meanwhile, `App.tsx` currently polls `GET /api/status` every 2.5 seconds via `store.poll()`.

## Goals / Non-Goals

**Goals:**
- Provide low-latency status updates via `/api/ws/status` when the target has sufficient socket and SRAM headroom.
- Guarantee zero regression to `/api/ws/audio` and REST/OTA responsiveness.
- Ensure the frontend works identically on both WebSocket-enabled and polling-only targets via transparent fallback.

**Non-Goals:**
- Multiplexing audio and status onto a single WebSocket (keeps `/api/ws/audio` binary G.711 framing simple and isolated).
- Supporting multiple simultaneous `/api/ws/status` browser tabs on tight-memory targets (a single active status WS worker or bounded worker pool is sufficient).

## Decisions

1. **Reuse `picoserve` `WebSocketUpgrade` for `/api/ws/status`:**
   - Because `picoserve` WebSocket support is already compiled and proven in `ws_audio.rs`, adding a `WsStatusSession` handler requires minimal new protocol code.
   - Inside `WsStatusSession`, `embassy_futures::select::select` waits on either a status-change notification signal (`Watch` / `Signal` bumped by `AccessController` / `DiagnosticsService`) or a 1.5s `Timer::after` tick, serializing `format_status_json` into a stack buffer and sending a text frame.

2. **Explicit C6 / S3 Socket & SRAM Budget Gating:**
   - Expanding `HttpServer` from 2 to 3+ `accept_worker`s increases `embassy-net` `StackResources` TCP sockets and per-worker RX/TX/HTTP buffers.
   - Gate `/api/ws/status` and the 3rd accept worker behind a compile-time Cargo feature or `HttpServerConfig` flag (e.g., `status-ws`), enabled on ESP32-S3/S31 and disabled on ESP32-C6 unless SRAM/socket benchmarks confirm >= 32 KiB free heap headroom during simultaneous Matter WebRTC + HTTP WS.

3. **Frontend `intercomStore.ts` Hybrid Transport:**
   - On mount, attempt `new WebSocket(\`${proto}//${host}/api/ws/status\`)`.
   - On `onmessage`, parse JSON into `setStatus(...)` and clear any active polling interval.
   - On `onerror` / `onclose`, start (or resume) the 2.5s `setInterval(poll, 2500)` fallback, with exponential backoff before retrying WebSocket (or stopping WS retries after N consecutive 404/rejection responses).

## Risks / Trade-offs

- **[Risk] Worker exhaustion from multiple open browser tabs:** Two open tabs could both hold `/api/ws/status` connections. → **Mitigation:** Track an `AtomicBool` / single-slot guard for `/api/ws/status` so a new status WS connection either replaces the old one or rejects with `503` (triggering polling fallback on the extra tab), preserving REST and `/api/ws/audio` workers.
- **[Risk] ESP32-C6 SRAM regression:** Adding a 3rd HTTP worker on C6 (~8–12 KiB buffers + `embassy-net` socket) could eat into the 12 KiB heap headroom needed during WebRTC calls. → **Mitigation:** Gate off on C6 by default; enable on S3/S31 where PSRAM and larger internal SRAM are available.
