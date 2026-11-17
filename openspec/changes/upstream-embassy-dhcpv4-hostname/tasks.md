# Tasks: Upstream Opt-in `dhcpv4-hostname` Feature in rs-matter-embassy

- [ ] 1.1 In `rs-matter-embassy`, cut clean feature branch `feat/dhcpv4-hostname` from `upstream/master`
- [ ] 1.2 Add `dhcpv4-hostname = ["embassy-net?/dhcpv4-hostname"]` to `rs-matter-embassy/Cargo.toml`
- [ ] 1.3 Port `create_enet_stack_with_hostname` and its unit tests to `rs-matter-embassy/src/enet.rs`
- [ ] 1.4 Add `#[cfg(feature = "dhcpv4-hostname")] pub const fn with_hostname` to `EmbassyEthernet` (`src/eth.rs`) and `EmbassyWifi` (`src/wireless/wifi.rs`)
- [ ] 1.5 Verify `cargo fmt --all -- --check`, `cargo clippy`, and unit tests both with and without `--features embassy-net,dhcpv4-hostname`
- [ ] 1.6 Open PR to `sysgrok/rs-matter-embassy`

## Workflow follow-up

- Submit PR to `sysgrok/rs-matter-embassy` repository.
- Archive the OpenSpec change with `openspec archive upstream-embassy-dhcpv4-hostname`.
