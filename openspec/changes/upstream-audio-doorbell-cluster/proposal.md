# Proposal: Upstream Audio Doorbell Device Type & AudioAvStreamHandler in rs-matter

## Why

The Matter 1.4 specification introduces the Audio Doorbell device type (`0x0141`) and Camera AV Stream Management cluster (`0x0551`). Upstream `project-chip/rs-matter` currently lacks support for these types. Contributing a clean, zero-heap-allocation, audio-only implementation to `project-chip/rs-matter` provides standard compliance for smart doorbells and intercoms across the embedded Rust ecosystem without the RAM overhead of video clusters.

## What Changes

- Add standard cluster ID `0x0551` and device type `0x0141` in `rs-matter-dm` and `rs-matter-dm-clusters`.
- Implement `AudioAvStreamHandler` with `Feature::AUDIO | Feature::SPEAKER` omitting video/snapshot state to fit embedded microcontrollers.
- Implement command dispatch for `AllocateAudioStream` and `DeallocateAudioStream` supporting 16 kHz / 48 kHz mono Opus profiles.
- Enforce monotonic stream ID allocation wrapping past `0xFFFF` and saturation-safe reference counting.
- Enforce standard Matter global cluster attributes (`0xFFF8` to `0xFFFD`) with zero runtime heap allocation.

## Capabilities

### New Capabilities
- `camera-av-stream-cluster`: Standard Matter 1.4 Camera AV Stream Management cluster (0x0551) and Audio Doorbell device type (0x0141) data model abstractions and command handling.

### Modified Capabilities
<!-- None -->

## Impact

- **Affected Repositories**:
  - `project-chip/rs-matter` (`rs-matter-dm`, `rs-matter-dm-clusters`)
- **API Surface**:
  - `DEV_TYPE_AUDIO_DOORBELL` in `rs_matter::dm::devices`.
  - `AudioAvStreamHandler` in `rs_matter::dm::clusters`.
- **Breaking Changes**: None. Additive data model features.
