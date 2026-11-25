# Spec Delta

## Purpose

Defines Git-tag-driven Semantic Versioning 2.0.0 metadata generation, strictly monotonic Matter numeric version encoding, and release tagging behavior for `intercom-matter` and `hvac-bridge` firmware builds.

## ADDED Requirements

### Requirement: Git-tag-driven SemVer resolution with Cargo fallback
The build system SHALL resolve `MAJOR.MINOR.PATCH` from the nearest reachable annotated or lightweight Git tag matching `v<MAJOR>.<MINOR>.<PATCH>`, falling back to `CARGO_PKG_VERSION` when no matching tag is reachable.

#### Scenario: Reachable release tag overrides Cargo.toml
- **WHEN** the repository HEAD is at or ahead of Git tag `v0.2.0`
- **THEN** the resolved base semantic version is `0.2.0`

#### Scenario: Untagged repository falls back to Cargo.toml
- **WHEN** no `vX.Y.Z` Git tag is reachable from HEAD and `CARGO_PKG_VERSION` is `0.1.0`
- **THEN** the resolved base semantic version is `0.1.0` and commit distance is counted from the root commit

### Requirement: SemVer string formatting for release and development builds
The build system SHALL format `SW_VER_STR` as clean `X.Y.Z` on exact clean tags, `X.Y.Z+<N>.g<hash>` when `N > 0` commits ahead of a tag, and append `.dirty` when the working tree has uncommitted changes, truncated or bounded to at most 64 bytes for Matter compatibility.

#### Scenario: Exact clean release tag
- **WHEN** HEAD points directly to `v0.2.0` (`N = 0`) and the working tree is clean
- **THEN** `SW_VER_STR` is `"0.2.0"`

#### Scenario: Post-tag clean development build
- **WHEN** HEAD is `5` commits ahead of `v0.2.0` at short hash `abc1234` and the working tree is clean
- **THEN** `SW_VER_STR` is `"0.2.0+5.gabc1234"`

#### Scenario: Dirty working tree on tag or post-tag
- **WHEN** the working tree has uncommitted changes at `0` commits ahead of `v0.2.0` at short hash `abc1234`
- **THEN** `SW_VER_STR` is `"0.2.0+0.gabc1234.dirty"`

#### Scenario: Matter 64-byte SoftwareVersionString bound
- **WHEN** `SW_VER_STR` is generated
- **THEN** its byte length MUST be at most 64 bytes

### Requirement: Strictly monotonic Matter u32 SoftwareVersion encoding
The build system SHALL compute `SW_VER` as `(major * 100_000_000) + (minor * 1_000_000) + (patch * 10_000) + commits_since_tag`, supporting `major` 0–42, `minor` 0–99, `patch` 0–99, and `commits_since_tag` 0–9999 without field overflow or collision.

#### Scenario: Patch >= 10 does not collide with minor
- **WHEN** the version is `0.1.10` at `0` commits since tag
- **THEN** `SW_VER` is `1_100_000`, which is strictly less than `0.2.0` (`2_000_000`)

#### Scenario: Post-tag commits increase monotonically until next release
- **WHEN** `v0.2.0` (`SW_VER = 2_000_000`) advances by 5 commits (`2_000_005`) and then tags `v0.2.1` (`2_010_000`)
- **THEN** `SW_VER` strictly increases across every step

#### Scenario: Out-of-range version components fail build
- **WHEN** `minor >= 100`, `patch >= 100`, `major > 42`, or `commits_since_tag >= 10_000`
- **THEN** the build script MUST fail with a clear diagnostic error rather than silently overflowing

### Requirement: Release tagging helper script
Each project SHALL provide a `scripts/release.py` CLI that verifies a clean Git working tree, updates `[workspace.package].version` in `Cargo.toml`, commits the bump, and creates an annotated `vX.Y.Z` Git tag.

#### Scenario: Dirty working tree blocks release
- **WHEN** `scripts/release.py` is invoked with uncommitted changes
- **THEN** it exits non-zero without modifying `Cargo.toml` or creating a tag

#### Scenario: Clean release bump and tag
- **WHEN** `scripts/release.py 0.2.0` is invoked on a clean working tree
- **THEN** `[workspace.package].version` in `Cargo.toml` is updated to `"0.2.0"`, committed, and annotated tag `v0.2.0` is created
