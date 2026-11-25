# Tasks

## 1. Shared `edf-build` Crate (`embedded-device-foundation`)

- [x] 1.1 Scaffold `../embedded-device-foundation/crates/edf-build/Cargo.toml` and `src/lib.rs` (std-only build helper) and verify workspace discovery with `mise exec -- cargo metadata --manifest-path ../embedded-device-foundation/Cargo.toml --no-deps > /dev/null`
- [x] 1.2 [RED] Write failing unit tests in `../embedded-device-foundation/crates/edf-build/src/lib.rs` asserting `parse_git_describe` handles `v2.0.0-0-gabc1234`, `v1.0.5-12-gdef5678`, and rejects non-SemVer tags, verifying failure with: `mise exec -- cargo test --manifest-path ../embedded-device-foundation/Cargo.toml -p edf-build --target x86_64-apple-darwin parse_git_describe`
- [x] 1.3 [GREEN] Implement `parse_git_describe` in `../embedded-device-foundation/crates/edf-build/src/lib.rs`, verifying test passes with: `mise exec -- cargo test --manifest-path ../embedded-device-foundation/Cargo.toml -p edf-build --target x86_64-apple-darwin parse_git_describe`
- [x] 1.4 [RED] Write failing unit tests in `../embedded-device-foundation/crates/edf-build/src/lib.rs` asserting `encode_sw_ver` computes `(major * 100_000_000) + (minor * 1_000_000) + (patch * 10_000) + commits_since_tag`, avoids `patch >= 10` collision (`0.1.10` < `0.2.0`), and panics/errors on out-of-range bounds (`major > 42`, `minor >= 100`, `patch >= 100`, `commits >= 10_000`), verifying failure with: `mise exec -- cargo test --manifest-path ../embedded-device-foundation/Cargo.toml -p edf-build --target x86_64-apple-darwin encode_sw_ver`
- [x] 1.5 [GREEN] Implement `encode_sw_ver` in `../embedded-device-foundation/crates/edf-build/src/lib.rs`, verifying test passes with: `mise exec -- cargo test --manifest-path ../embedded-device-foundation/Cargo.toml -p edf-build --target x86_64-apple-darwin encode_sw_ver`
- [x] 1.6 [RED] Write failing unit tests in `../embedded-device-foundation/crates/edf-build/src/lib.rs` asserting `format_sw_ver_str` outputs `"2.0.0"` on clean exact tag, `"2.0.0+5.gabc1234"` on post-tag clean build, `"2.0.0+0.gabc1234.dirty"` on dirty tree, and enforces the 64-byte Matter `SoftwareVersionString` limit, verifying failure with: `mise exec -- cargo test --manifest-path ../embedded-device-foundation/Cargo.toml -p edf-build --target x86_64-apple-darwin format_sw_ver_str`
- [x] 1.7 [GREEN] Implement `format_sw_ver_str` and `emit_version_env(prefix: &str, pkg_version: &str)` (including `cargo:rerun-if-changed` for `HEAD`, `index`, `refs/tags`, `packed-refs`, and `build.rs`) in `../embedded-device-foundation/crates/edf-build/src/lib.rs`, verifying all `edf-build` tests pass with: `mise exec -- cargo test --manifest-path ../embedded-device-foundation/Cargo.toml -p edf-build --target x86_64-apple-darwin`

## 2. Wire `intercom-firmware` and `hvac-firmware` to `edf-build`

- [x] 2.1 Add `edf-build = { path = "../embedded-device-foundation/crates/edf-build" }` to `Cargo.toml` (`[workspace.dependencies]`) and `crates/intercom-firmware/Cargo.toml` (`[build-dependencies]`), and update `crates/intercom-firmware/build.rs` to call `edf_build::emit_version_env("INTERCOM", env!("CARGO_PKG_VERSION"))`, verifying host tests pass with: `mise exec -- cargo test -p intercom-firmware --lib --target x86_64-apple-darwin`
- [x] 2.2 Verify `intercom-firmware` cross-compilation succeeds with `edf-build` by running: `mise exec -- cargo check -p intercom-firmware`
- [x] 2.3 Add `edf-build = { path = "../embedded-device-foundation/crates/edf-build" }` to `../hvac-bridge/Cargo.toml` (`[workspace.dependencies]`) and `../hvac-bridge/crates/hvac-firmware/Cargo.toml` (`[build-dependencies]`), and update `../hvac-bridge/crates/hvac-firmware/build.rs` to call `edf_build::emit_version_env("HVAC", env!("CARGO_PKG_VERSION"))`, verifying cross-compilation succeeds with: `mise exec -- cargo check --manifest-path ../hvac-bridge/Cargo.toml -p hvac-firmware`

## 3. Release Tagging Helper (`scripts/release.py`)

- [x] 3.1 [RED] Write failing Python unit tests in `scripts/test_release.py` asserting `release.py` rejects dirty working trees, rejects invalid SemVer strings, updates `[workspace.package].version` in `Cargo.toml`, and constructs `git tag -a vX.Y.Z`, verifying failure with: `python3 -m unittest scripts/test_release.py`
- [x] 3.2 [GREEN] Implement `scripts/release.py` (supporting `--dry-run`, clean-tree check, `Cargo.toml` version update, commit, and annotated `vX.Y.Z` tag), verifying tests pass with: `python3 -m unittest scripts/test_release.py`
- [x] 3.3 Copy `scripts/release.py` and `scripts/test_release.py` to `../hvac-bridge/scripts/` and verify tests pass with: `python3 -m unittest ../hvac-bridge/scripts/test_release.py`

## 4. Baseline Version Tags & Final Verification

- [x] 4.1 Update `[workspace.package].version` to `"2.0.0"` on `intercom-matter` `main` (and `"1.0.0"` on `v1`, `"0.1.0"` on `hvac-bridge` `main`), create annotated baseline tags (`v2.0.0`, `v1.0.0`, `v0.1.0`), and verify `openspec validate semantic-versioning --strict` and firmware builds emit the expected `SW_VER_STR` and `SW_VER` values.

## Workflow follow-up

- Archive the change with `/openspec-archive-change` after implementation and baseline tagging are complete.
