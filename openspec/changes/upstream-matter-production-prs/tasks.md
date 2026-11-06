# Tasks: Upstream Production PRs for rs-matter and Downstream Adapter Integration

## 0. Upstream Sync & Clean Feature Branch Setup

- [x] 0.1 In `rs-matter`, fetch `upstream` (`git fetch upstream`) and cut clean feature branches `feat/audio-doorbell-cluster` and `feat/matter-over-tcp` directly from `upstream/main`, verifying the base commit matches `upstream/main`
- [x] 0.2 Verify `rs-matter-embassy` upstream `master` includes `enet-extra-sockets-2`, confirming no external PR is required for socket pool sizing

## 1. PR 1: Audio Doorbell Device Type & AudioAvStreamHandler in project-chip/rs-matter

- [x] 1.1 Port `AudioAvStreamHandler` into the modular `project-chip/rs-matter` workspace under `rs-matter-dm-clusters` (or `rs-matter-dm`), registering `DEV_TYPE_AUDIO_DOORBELL` (`0x0141`) in `rs-matter-dm/src/devices.rs`, and verify compilation with `cargo check -p rs-matter-dm -p rs-matter-dm-clusters`
- [x] 1.2 Implement command dispatch for `AllocateAudioStream` and `DeallocateAudioStream` supporting 16 kHz / 48 kHz mono Opus profiles, rejecting unsupported codecs with `DynamicConstraintError`, and verify via unit test
- [x] 1.3 Implement monotonic stream ID allocation wrapping past `0xFFFF` skipping active IDs, and saturation-guarded reference counting returning `ResourceExhausted` when exceeding 255 leases
- [x] 1.4 Add unit tests verifying global system attributes (`0xFFF8` through `0xFFFD`), feature map bits (`AUDIO | SPEAKER`), and deallocation rejection during active WebRTC leases; verify all tests pass with `cargo test -p rs-matter-dm-clusters --lib`
- [x] 1.5 Run `cargo clippy --all-targets --all-features -- -D warnings` on `rs-matter`, format code with `cargo fmt`, and create PR commit on `feat/audio-doorbell-cluster` ready for upstream review

## 2. PR 2: Embedded Matter-over-TCP Integration & Operational Runner in project-chip/rs-matter

- [ ] 2.1 Implement or reuse `no_std` Matter TCP frame parsing for the embedded listener using a 4-byte little-endian length prefix; test fragmented and back-to-back frames. Preserve the existing host framing in `rs-matter-transport`
- [ ] 2.2 Add TCP accept/listen support to the `rs-matter-stack` network abstraction and wire its port 5540 listener into the operational runner, cooperatively with UDP MRP so TCP activity cannot starve UDP processing
- [ ] 2.3 Set the operational device's `tcp_supported` flag when the embedded TCP server is enabled, reusing the existing mDNS encoder to advertise `T=6`; verify the resulting operational TXT record
- [ ] 2.4 Add a `WifiOnly` runner path in `rs-matter-stack/src/wireless/wifi.rs` that does not require or initialize BLE GATT for a provisioned device
- [ ] 2.5 Add integration tests covering single-connection concurrency control, idle connection timeouts, and connection teardown
- [ ] 2.6 Run `cargo clippy` and `cargo test` across `rs-matter-transport` and `rs-matter-stack`, format code, and create PR commit on `feat/matter-over-tcp` ready for upstream review

## 3. Downstream Adapter Integration & Embassy Zero-Change Wiring

- [ ] 3.1 Configure `rs-matter-embassy` dependency to consume upstream `sysgrok/rs-matter-embassy` directly with `features = ["enet-extra-sockets-2"]`, verifying compilation and socket headroom for REST and WebSocket servers
- [ ] 3.2 Implement `IntercomWifiDriver` in `intercom-adapter-matter/src/wireless.rs` supporting 802.11ax (Wi-Fi 6) negotiation, fast AP scanning (`ScanMethod::Fast`), and RSSI deduplication
- [ ] 3.3 Implement adapter-level on-demand TCP socket buffer allocation and teardown in `intercom-adapter-matter`, freeing 16–24 KiB of SRAM during idle standby

## 4. End-to-End Verification & Workspace Cleanup

- [ ] 4.1 Update `intercom-matter` workspace dependencies in `Cargo.toml` to point to the new upstream PR branches and verify all host tests pass (`cargo test -p intercom-core -p intercom-adapter-http -p intercom-pairing-tool --target x86_64-apple-darwin`)
- [ ] 4.2 Build release DevKit firmware with `cargo build-devkit` and verify the binary links cleanly within the 48 KiB stack budget

## Workflow follow-up

- Submit PR 1 to `project-chip/rs-matter` repository.
- Submit PR 2 to `project-chip/rs-matter` repository.
- Archive the OpenSpec change with `openspec archive upstream-matter-production-prs` once upstream reviews and local integrations conclude.
