# Design: Upstream MLDv1 & IPv6 Neighbor Learning Compatibility Wrappers in rs-matter-embassy

## Context

`intercom-matter` relies on `1c94597` in `rs-matter-embassy/src/enet.rs` (`+1113` lines) to bridge `smoltcp`/`embassy-net` IPv6 behavior with consumer Wi-Fi APs and Apple Home hubs. Because this is the largest standalone diff in the fork, separating it into its own clean, well-tested PR allows independent review by `sysgrok` without blocking smaller PRs.

## Decisions

### 1. Transparent `embassy_net::driver::Driver` Wrappers
- **Decision**: Implement `Mldv1ConvertingDriver<D>` and `NeighborLearningDriver<D>` as `Driver` decorators composed via `pub type MatterEnetDriver<D>`.
- **Rationale**: Keeps `smoltcp` and `embassy-net` unmodified while allowing each shim to be toggled independently via Cargo features (`mld-v1-compat`, `nd-learn-from-frames`).

### 2. Feature Default Discussion
- **Decision**: Confirm with upstream whether `mld-v1-compat` and `nd-learn-from-frames` should be opt-in or in `default` features, and isolate helper tests cleanly in `enet.rs` (or a dedicated submodule `enet/compat.rs` if requested during review).
- **Rationale**: Keeping the wrappers modular reduces review friction on a ~1,100-line addition.

## Migration Plan

1. Cut clean feature branch `feat/mld-nd-compat` from `upstream/master` in `sysgrok/rs-matter-embassy`.
2. Port `1c94597` (`Cargo.toml` features + `src/enet.rs` wrappers and tests).
3. Verify all 4 feature combinations (`none`, `mld-v1-compat`, `nd-learn-from-frames`, both) compile and pass tests + `clippy` + `fmt`.
4. Submit PR to `sysgrok/rs-matter-embassy` (open to splitting into a submodule if upstream prefers).
5. Archive change with `openspec archive upstream-embassy-mld-nd-compat`.
