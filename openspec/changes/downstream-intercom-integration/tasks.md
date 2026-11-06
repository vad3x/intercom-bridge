# Tasks: Downstream Intercom Bridge Integration & Verification

- [ ] 1.1 Update `intercom-matter` workspace `Cargo.toml` dependencies to point to the new upstream PR branches/releases, removing local directory path patches
- [ ] 1.2 Run host test suite and verify all 107 tests pass (`cargo test -p intercom-core -p intercom-adapter-http -p intercom-pairing-tool --target x86_64-apple-darwin`)
- [ ] 1.3 Build release DevKit firmware with `cargo build-devkit` and verify the binary links cleanly within the 48 KiB stack budget
- [ ] 1.4 Perform smoke test on ESP32-C6 hardware to verify operational Matter commissioning and audio streaming

## Workflow follow-up

- Archive the OpenSpec change with `openspec archive downstream-intercom-integration`.
