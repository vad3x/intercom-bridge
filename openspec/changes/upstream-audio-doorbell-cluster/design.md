# Design: Upstream Audio Doorbell Device Type & AudioAvStreamHandler in rs-matter

## Context

Upstream `project-chip/rs-matter` refactored its data model into modular crates (`rs-matter-dm-core`, `rs-matter-dm`, `rs-matter-dm-clusters`). While Matter 1.4 defines both Camera and Audio Doorbell profiles, full camera clusters consume over 4 KiB of SRAM per endpoint. `AudioAvStreamHandler` provides a dedicated, lightweight implementation focused on two-way audio streaming.

## Decisions

### 1. Dedicated `AudioAvStreamHandler` vs Monolithic Camera Handler
- **Decision**: Introduce a dedicated `AudioAvStreamHandler` in `rs-matter-dm-clusters` exposing `Feature::AUDIO | Feature::SPEAKER` without video/snapshot state.
- **Rationale**: Minimal RAM footprint (<200 bytes) suitable for resource-constrained microcontrollers. Full IDL compliance for `DEV_TYPE_AUDIO_DOORBELL` (`0x0141`).

### 2. Stream ID Wrapping and Reference Count Guards
- **Decision**: Implement monotonic `stream_id` allocation wrapping from `0xFFFF` to `1` (skipping active IDs) and saturation-guarded reference counting returning `ResourceExhausted` when exceeding 255 leases.
- **Rationale**: Matter controllers repeatedly open and close streams. Wrapping prevents exhaustion; reference guards prevent integer overflow or premature deallocation during active sessions.

## Migration Plan

1. Cut clean feature branch `feat/audio-doorbell-cluster` from `upstream/main` in `project-chip/rs-matter`.
2. Port implementation and comprehensive unit tests.
3. Submit PR to `project-chip/rs-matter`.
4. Archive change with `openspec archive upstream-audio-doorbell-cluster`.
