# Tasks: Upstream Configurable App Sockets & Stack Access in rs-matter-embassy

- [ ] 1.1 In `rs-matter-embassy`, cut clean feature branch `feat/app-sockets-and-stack-access` from `upstream/master`
- [ ] 1.2 Make `ENET_EXTRA_TCP_SOCKETS` and `ENET_EXTRA_UDP_SOCKETS` configurable (supporting >= 2 TCP and >= 2 UDP spare sockets in `ENET_MIN_SOCKET_SET`)
- [ ] 1.3 Implement upstream-friendly `embassy_net::Stack` access (`global_stack` / `wait_global_stack` or callback/signal hook) without `alloc`
- [ ] 1.4 Verify `cargo fmt --all -- --check`, `cargo clippy`, and `cargo test` with default and expanded socket configurations
- [ ] 1.5 Open PR to `sysgrok/rs-matter-embassy`

## Workflow follow-up

- Submit PR to `sysgrok/rs-matter-embassy` repository.
- Update `intercom-firmware` (`main.rs`, `hardware_info.rs`) if the `Stack` access API changes during upstream review.
- Archive the OpenSpec change with `openspec archive upstream-embassy-app-sockets`.
