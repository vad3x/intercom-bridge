# Proposal

## Why

The web dashboard currently polls `GET /api/status` every 2.5 seconds (`App.tsx`), which introduces up to 2.5s of UI latency on state transitions (door unlock, ring, call state) and generates continuous HTTP connection churn. Replacing polling with a push-based WebSocket stream can deliver near-instantaneous status updates, provided it does not starve REST or `/api/ws/audio` on constrained targets like ESP32-C6.

## What Changes

- Add an optional `GET /api/ws/status` WebSocket endpoint in `intercom-adapter-http` using `picoserve::response::ws::WebSocketUpgrade`, pushing status JSON on state changes and on a 1–2s periodic telemetry tick.
- Gate `/api/ws/status` behind explicit TCP socket and SRAM budget requirements: `HttpServer` currently runs 2 accept workers (one of which is held by `/api/ws/audio` during Push-to-Talk), so status WebSocket requires expanding to 3+ accept workers and verifying `embassy-net` socket/SRAM headroom (enabled on ESP32-S3/PSRAM targets, gated/disabled on ESP32-C6 if SRAM or socket budget is exceeded).
- Update `intercomStore.ts` in the frontend to attempt `/api/ws/status` first and automatically fall back to 2.5s `GET /api/status` HTTP polling whenever the WebSocket is unavailable, rejected, or disconnected.

## Capabilities

### New Capabilities
- `http/status-websocket`: Push-based WebSocket status streaming (`/api/ws/status`) with state-change + periodic updates, target socket/SRAM budget gating, and transparent HTTP polling fallback.

### Modified Capabilities
<!-- No existing specs in openspec/specs/ are modified. -->

## Impact

- `crates/intercom-adapter-http`: `src/server.rs` (worker count / `AppState` / `/api/ws/status` route), `src/config.rs` (optional feature/config gating).
- `crates/intercom-firmware`: `embassy-net` `StackResources` socket count and SRAM budget verification per target (ESP32-C6 vs. ESP32-S3/S31).
- `crates/intercom-adapter-http/frontend`: `src/store/intercomStore.ts` and `src/App.tsx` (WebSocket client + polling fallback).
