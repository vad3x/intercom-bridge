# Tasks: Embedded Matter-over-TCP Integration & Operational Runner in rs-matter

- [ ] 2.1 In `rs-matter`, verify clean feature branch `feat/matter-over-tcp` cut from `upstream/main`
- [ ] 2.2 Implement or reuse `no_std` Matter TCP frame parsing for the embedded listener using a 4-byte little-endian length prefix; test fragmented and back-to-back frames. Preserve existing host framing in `rs-matter-transport`
- [ ] 2.3 Add TCP accept/listen support to the `rs-matter-stack` network abstraction and wire its port 5540 listener into the operational runner cooperatively with UDP MRP
- [ ] 2.4 Set the operational device's `tcp_supported` flag when the embedded TCP server is enabled, reusing the existing mDNS encoder to advertise `T=6`; verify the resulting operational TXT record
- [ ] 2.5 Add a `WifiOnly` runner path in `rs-matter-stack/src/wireless/wifi.rs` that does not require or initialize BLE GATT for a provisioned device
- [ ] 2.6 Add integration tests covering single-connection concurrency control, idle connection timeouts, and connection teardown
- [ ] 2.7 Run `cargo clippy` and `cargo test` across `rs-matter-transport` and `rs-matter-stack`, format code, and create PR commit on `feat/matter-over-tcp` ready for upstream review

## Workflow follow-up

- Submit PR to `project-chip/rs-matter` repository.
- Archive the OpenSpec change with `openspec archive upstream-matter-over-tcp`.
