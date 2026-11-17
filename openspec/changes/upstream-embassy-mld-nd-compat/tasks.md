# Tasks: Upstream MLDv1 & IPv6 Neighbor Learning Compatibility Wrappers in rs-matter-embassy

- [ ] 1.1 In `rs-matter-embassy`, cut clean feature branch `feat/mld-nd-compat` from `upstream/master`
- [ ] 1.2 Add `mld-v1-compat` and `nd-learn-from-frames` features to `rs-matter-embassy/Cargo.toml`
- [ ] 1.3 Port `Mldv1ConvertingDriver`, `NeighborLearningDriver`, `MatterEnetDriver`, and unit tests from `1c94597` to `rs-matter-embassy/src/enet.rs`
- [ ] 1.4 Verify `cargo fmt --all -- --check`, `cargo clippy`, and `cargo test` across all 4 combinations of `mld-v1-compat` and `nd-learn-from-frames`
- [ ] 1.5 Open PR to `sysgrok/rs-matter-embassy`

## Workflow follow-up

- Submit PR to `sysgrok/rs-matter-embassy` repository.
- Archive the OpenSpec change with `openspec archive upstream-embassy-mld-nd-compat`.
