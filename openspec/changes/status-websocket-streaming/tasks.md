# Tasks

## 1. Concurrency & Socket Budget Audit

- [ ] 1.1 Measure per-worker SRAM footprint (`TCP_RX_BUFFER_SIZE + TCP_TX_BUFFER_SIZE + HTTP_BUFFER_SIZE`) and `embassy-net` `StackResources` socket count on ESP32-C6 vs. ESP32-S3
- [ ] 1.2 Add a Cargo feature or `HttpServerConfig` toggle (e.g., `status-ws`) to enable a 3rd `accept_worker` on targets that meet the SRAM/socket budget and gate it off on ESP32-C6 if headroom is insufficient

## 2. Backend `/api/ws/status` Endpoint (`crates/intercom-adapter-http`)

- [ ] 2.1 Add a state-change notification signal (`Signal` or `Watch` generation counter) so `WsStatusSession` wakes immediately on door/bus/call/system state changes
- [ ] 2.2 Implement `WsStatusSession` (`WebSocketCallback`) in `crates/intercom-adapter-http` sending initial JSON, state-change updates, and 1.5s periodic ticks, with single-active-client guard so REST and `/api/ws/audio` are never starved
- [ ] 2.3 Wire `GET /api/ws/status` into `build_router` when `status-ws` is enabled

## 3. Frontend WebSocket + Polling Fallback (`crates/intercom-adapter-http/frontend`)

- [ ] 3.1 Update `intercomStore.ts` to connect to `/api/ws/status`, update `status` on incoming JSON frames, and fall back to 2.5s `fetchStatus()` polling on close/error
- [ ] 3.2 Update `App.tsx` lifecycle (`onMount` / `onCleanup`) to start the hybrid WS/polling store and clean up on unmount
- [ ] 3.3 Run `cargo fe-check && cargo fe-build` and verify both WebSocket-enabled and polling-fallback modes
