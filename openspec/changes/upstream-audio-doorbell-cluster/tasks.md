# Tasks: Upstream Audio Doorbell Device Type & AudioAvStreamHandler in rs-matter

- [x] 1.1 In `rs-matter`, fetch `upstream` (`git fetch upstream`) and cut clean feature branch `feat/audio-doorbell-cluster` directly from `upstream/main`
- [x] 1.2 Port `AudioAvStreamHandler` into `rs-matter-dm-clusters` and register `DEV_TYPE_AUDIO_DOORBELL` (`0x0141`) in `rs-matter-dm/src/devices.rs`, verifying compilation with `cargo check -p rs-matter-dm -p rs-matter-dm-clusters`
- [x] 1.3 Implement command dispatch for `AllocateAudioStream` and `DeallocateAudioStream` supporting 16 kHz / 48 kHz mono Opus profiles, rejecting unsupported codecs with `DynamicConstraintError`
- [x] 1.4 Implement monotonic stream ID allocation wrapping past `0xFFFF` skipping active IDs, and saturation-guarded reference counting returning `ResourceExhausted` when exceeding 255 leases
- [x] 1.5 Add unit tests verifying global system attributes (`0xFFF8` through `0xFFFD`), feature map bits (`AUDIO | SPEAKER`), and deallocation rejection during active WebRTC leases; verify all tests pass with `cargo test -p rs-matter-dm-clusters --lib`
- [x] 1.6 Run `cargo clippy --all-targets --all-features -- -D warnings` on `rs-matter`, format code with `cargo fmt`, and create PR commit on `feat/audio-doorbell-cluster` ready for upstream review

## Workflow follow-up

- Submit PR to `project-chip/rs-matter` repository.
- Archive the OpenSpec change with `openspec archive upstream-audio-doorbell-cluster`.
