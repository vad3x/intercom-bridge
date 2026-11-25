# Design

## Context

See `proposal.md` for motivation. Both `intercom-matter` (`crates/intercom-firmware/build.rs`) and `hvac-bridge` (`../hvac-bridge/crates/hvac-firmware/build.rs`) currently duplicate identical git-query and version-encoding logic and depend on `embedded-device-foundation` (`edf-*`). Neither repository currently uses `vX.Y.Z` release tags (`intercom-matter` has 124 commits and no SemVer tags; `hvac-bridge` has 3 commits and no tags).

## Goals / Non-Goals

**Goals:**
- Centralize SemVer parsing, Git tag/describe resolution, `SW_VER_STR` formatting, and `SW_VER` `u32` calculation in a new `edf-build` crate inside `embedded-device-foundation`, with full host unit test coverage.
- Preserve the existing `INTERCOM_*` and `HVAC_*` `cargo:rustc-env` variable names so `version.rs`, `main.rs`, and `hardware_info.rs` require no consumer API changes.
- Guarantee strictly monotonic `SW_VER` values across the transition from the legacy formula (`0.1.0` + 124 commits = `100_124` in `intercom-matter`, `100_003` in `hvac-bridge`) to the new formula (`0.1.0` = `1_000_000`, `0.2.0` = `2_000_000`).
- Provide a repeatable `scripts/release.py` helper in both repositories to keep `Cargo.toml` and `vX.Y.Z` tags synchronized.

**Non-Goals:**
- Adding SemVer pre-release tag (`-rc.1`, `-alpha.1`) numeric ordering to `SW_VER`.
- Implementing OTA downgrade protection or version comparison inside `edf-ota` (Matter OTA requestor / host tools consume `SW_VER` as-is).
- Initializing OpenSpec in `hvac-bridge` (changes to `hvac-bridge` are executed as part of this cross-repo plan).

## Decisions

### 1. Shared `edf-build` crate in `embedded-device-foundation`
- **Decision:** Create `../embedded-device-foundation/crates/edf-build` (std-only build helper, no embedded dependencies) exposing `emit_version_env(prefix: &str, pkg_version: &str)` and pure testable functions `resolve_version(...)` / `encode_sw_ver(...)`.
- **Rationale:** Eliminates duplicate `build.rs` logic across `intercom-firmware` and `hvac-firmware` and allows pure unit testing of edge cases (untagged repos, dirty trees, `patch >= 10`, overflow bounds, 64-byte truncation) via `cargo test -p edf-build`.
- **Alternatives considered:** Keeping duplicate `build.rs` files in each repo (rejected due to drift risk).

### 2. Git tag resolution via `git describe --tags --match "v[0-9]*.[0-9]*.[0-9]*" --long --abbrev=7`
- **Decision:** Query `git describe` for the nearest `vX.Y.Z` tag. Parse `<tag>-<N>-g<hash>` when a tag exists. When `git describe` fails (untagged repo), fall back to `pkg_version` (`CARGO_PKG_VERSION`) and `git rev-list --count HEAD` + `git rev-parse --short=7 HEAD`.
- **Rationale:** `git describe --long` gives exact `commits_since_tag` (`0` on exact tag) and short hash in a single deterministic command.
- **Dirty handling:** If `git status --porcelain` is non-empty, always include `+<N>.g<hash>.dirty` even when `N == 0`, so a dirty build on a tagged commit never masquerades as a clean release `"X.Y.Z"`.

### 3. Numeric `SW_VER` encoding: `major * 100_000_000 + minor * 1_000_000 + patch * 10_000 + commits_since_tag`
- **Decision:** Base-10 packed `u32` with 2-digit `minor`, 2-digit `patch`, and 4-digit `commits_since_tag`.
- **Migration check:** Current legacy `intercom-matter` builds report `100_124`; new `v1.0.0` (`v1` branch) reports `100_000_000` (`> 100_124`), `v2.0.0` (`main` branch) reports `200_000_000` (`> 100_000_000`), and `hvac-bridge` `v0.1.0` reports `1_000_000` (`> 100_003`), so Apple Home / Matter controllers see a strictly increasing version on upgrade.
- **Bounds:** `build.rs` panics if `major > 42`, `minor >= 100`, `patch >= 100`, or `commits_since_tag >= 10_000` to prevent silent `u32` overflow or field collision.

### 4. `cargo:rerun-if-changed` for tags and HEAD
- **Decision:** In addition to `HEAD`, `index`, and `build.rs`, emit `cargo:rerun-if-changed=<git_dir>/refs/tags` (and `packed-refs` if present) so creating a new `vX.Y.Z` tag triggers `build.rs` without needing `cargo clean`.

## Risks / Trade-offs

- **Shallow Git clones without tags** → Falls back to `CARGO_PKG_VERSION` + commit count; `release.py` keeps `Cargo.toml` synced with the latest tag so the fallback version remains accurate.
- **Cross-repo edits (`embedded-device-foundation`, `intercom-matter`, `hvac-bridge`)** → All three repos are local siblings; `tasks.md` sequences `edf-build` first, then `intercom-matter`, then `hvac-bridge`.
