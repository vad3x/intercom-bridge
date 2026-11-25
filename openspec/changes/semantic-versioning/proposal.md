# Proposal

## Why

Both `intercom-matter` and `hvac-bridge` currently duplicate build-time version generation in their `build.rs` scripts, relying on static `Cargo.toml` versions and total repository commit counts. The current numeric formula `(major * 10_000_000) + (minor * 100_000) + (patch * 10_000) + commit_count` overflows into `minor` as soon as `patch >= 10`, does not reset commit counts at release boundaries, and cannot distinguish clean tagged releases from post-tag development builds.

## What Changes

- Introduce a shared `edf-build` crate in `embedded-device-foundation` (`../embedded-device-foundation/crates/edf-build`) that resolves Semantic Versioning 2.0.0 metadata from Git tags (`vX.Y.Z`) with fallback to `CARGO_PKG_VERSION` when untagged.
- Compute clean SemVer strings on exact clean tags (`X.Y.Z`) and `git describe`-style build metadata on post-tag or dirty builds (`X.Y.Z+N.g<hash>` or `X.Y.Z+N.g<hash>.dirty`), enforcing the Matter 64-byte `SoftwareVersionString` limit.
- Replace the colliding numeric `SW_VER` formula with `(major * 100_000_000) + (minor * 1_000_000) + (patch * 10_000) + commits_since_tag` (supporting `major` 0–42, `minor` 0–99, `patch` 0–99, and `commits_since_tag` 0–9999) so Matter `SoftwareVersion` remains strictly monotonic and collision-free.
- Update `crates/intercom-firmware/build.rs` and `../hvac-bridge/crates/hvac-firmware/build.rs` to delegate version resolution to `edf-build`.
- Add a shared release helper script (`scripts/release.py` in `intercom-matter` and `hvac-bridge`, or invoked from either repo) that validates a clean working tree, bumps `[workspace.package].version` in `Cargo.toml`, and creates an annotated `vX.Y.Z` Git tag.
- Tag initial baseline releases (`v2.0.0` on `main` and `v1.0.0` on `v1` for `intercom-matter`; `v0.1.0` on `main` for `hvac-bridge`).

## Capabilities

### New Capabilities
- `firmware-versioning`: Git-tag-driven Semantic Versioning 2.0.0 string formatting, strictly monotonic Matter `u32` `SoftwareVersion` encoding, and release tagging workflow shared across `intercom-matter` and `hvac-bridge`.

### Modified Capabilities

## Impact

- `../embedded-device-foundation/Cargo.toml` and new crate `../embedded-device-foundation/crates/edf-build`
- `crates/intercom-firmware/Cargo.toml` and `crates/intercom-firmware/build.rs`
- `../hvac-bridge/crates/hvac-firmware/Cargo.toml` and `../hvac-bridge/crates/hvac-firmware/build.rs`
- `scripts/release.py` and `../hvac-bridge/scripts/release.py`
- Matter `BasicInformation` (`sw_ver`, `sw_ver_str`), HTTP `/api/status`, and serial boot banner output
