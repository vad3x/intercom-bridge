# Proposal

## Why

The ESP32-S31 migration has compile-time and host-side validation, but S31-specific timing, sustained streaming, PSRAM behavior, and OTA rollback still require a physical S31 board. Tracking those checks separately keeps the completed migration implementation distinct from the hardware acceptance work.

## What Changes

- Define repeatable on-device checks for Core 1 Opus latency and stack safety, sustained WebRTC listening, memory stability, and S31 OTA signing and rollback.
- Record the firmware revision, board and flash configuration, serial evidence, and status API output for each run.

## Capabilities

### New Capabilities

None. This change records hardware validation work and does not change system behavior.

### Modified Capabilities

None.

## Impact

The checks exercise the S31 firmware on physical hardware, including `intercom-adapter-webrtc` benchmark logging, the firmware memory status API, the serial monitor, the Matter WebRTC Listen path, and signed OTA deployment.
