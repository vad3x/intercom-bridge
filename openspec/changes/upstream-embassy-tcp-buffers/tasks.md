# Tasks: Upstream On-Demand TCP Buffers in rs-matter-embassy

- [ ] 2.1 Coordinate with `upstream-matter-over-tcp` to verify the TCP network trait signature in `rs-matter-stack`
- [ ] 2.2 Cut clean feature branch `feat/embassy-tcp-buffers` in `sysgrok/rs-matter-embassy`
- [ ] 2.3 Implement on-demand allocation of TCP socket RX/TX buffers in `rs-matter-embassy/src/eth.rs` and `rs-matter-embassy/src/enet.rs`
- [ ] 2.4 Add unit test verifying buffer pool is allocated on demand and not pre-reserved at context creation
- [ ] 2.5 Verify compilation on host and `riscv32imac-unknown-none-elf`
- [ ] 2.6 Run `cargo clippy` and `cargo fmt`, creating PR commit on `feat/embassy-tcp-buffers` ready for upstream review

## Workflow follow-up

- Submit PR to `sysgrok/rs-matter-embassy` repository.
- Archive the OpenSpec change with `openspec archive upstream-embassy-tcp-buffers`.
