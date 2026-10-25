# Local foundation integration

The host pairing tool and `intercom-core` consume crates from a sibling checkout
of `embedded-device-foundation`. The latest integration verification used
Foundation HEAD `51087f4957c950670e6538ec056c11cb74ecaecb` plus the local working
tree patch with SHA-256
`f2bf66d264e40b4517950b9e9d9e142fee8a8accc075902f087aca532d2b9042`.
The source checkout used here is
`/Users/vadim/personal/projects/embedded-device-foundation`.

For a normal pair of sibling repositories, place `embedded-device-foundation`
beside `intercom-matter`. For the isolated
`/Users/vadim/.codex/worktrees/dependency-alignment/intercom-matter` worktree,
create the absent sibling entry as a symlink to the foundation checkout:

```sh
ln -s /Users/vadim/personal/projects/embedded-device-foundation \
  /Users/vadim/.codex/worktrees/dependency-alignment/embedded-device-foundation
```

Only create that link when the destination does not already exist. The tracked
Cargo paths are relative to each consumer repository, so an equivalent sibling
layout is required on every build machine.

This is an unpinned local path dependency. Cargo.lock records the local
package versions and dependency graph, but it does not fix the foundation Git
revision. The verified working tree includes seven modified logger/dashboard
files, so the HEAD commit alone does not reproduce the shared APIs used here.
Preserve or apply the exact Foundation patch above when reproducing the latest
logger/dashboard results, and review future Foundation changes before updating
this integration. Earlier sections below retain their original revision values
as historical stage records.

## Pairing generation

`intercom-pairing-tool` consumes `edf-pairing-tool` and calls
`edf_pairing_tool::generate` with the existing Intercom identity: prefix `IC`,
hostname `intercom-bridge`, VID `0xfff1`, and PID `0x8001`.

The Intercom CLI retains its argument parsing, permissive MAC parsing, serial
discovery, and terminal QR rendering. Generated credentials and JSON formatting
come from the foundation crate. Host coverage includes unit tests for the shared
boundary plus executable checks for `--mac`, `-m`, positional MAC, `--json`,
help, and invalid short MAC.

## Lifecycle, ecosystem, and reset contracts

`intercom-core` consumes `edf-core` and exposes the foundation contracts through
the existing `intercom_core` module paths:

- `events` re-exports `SystemState`, `SystemStateWatch`, `SystemStateReceiver`,
  `SystemStateSender`, and `STATE_WATCH_RECEIVERS`.
- `domain::ecosystem` re-exports the ecosystem value objects and capacity
  constants.
- `domain::reset` re-exports `ResetScope` and `classify_reset_hold`.
- `ports::ecosystem` re-exports `EcosystemInfoDriver`.
- `ports::reset` re-exports `ResetScope`, `ResetTriggerDriver`, and
  `SystemResetDriver`.
- `service::reset` re-exports `ResetService`.
- `config` re-exports `FACTORY_RESET_REBOOT_DELAY_MS` (2000 ms).

Because these are re-exports of the same types rather than parallel local
definitions, downstream adapters implement the foundation traits directly and
no orphan implementations are required. Public consumer module paths and
lifecycle variants are unchanged; the watch receiver capacity remains 3,
`RING_HOLD_MS` remains 15000 ms, and `AUTO_RELOCK_SEC` remains 4 seconds.

One source-level API delta is intentional: the foundation `ResetService` carries
a private `reboot_delay` field and a `with_reboot_delay` builder. Construct it
with `ResetService::new(driver, reset)` (optionally followed by
`with_reboot_delay(...)`); struct-literal construction through the old local
`ResetService { driver, reset }` form is no longer supported. No in-repository
consumer used literal construction in this stage.

The following remain product-owned in this stage:

- Access, ring, command-handle, and commissioning policy.
- Diagnostics domain types, service, and hardware port.
- OTA port and driver error mapping.
- Matter, GPIO, dashboard, and host-script composition.

## Verification

Host test, Clippy, and formatting commands used the explicit
`x86_64-apple-darwin` target, Cargo through mise, and `--locked --offline`.
The MCU build row uses the `riscv32imac-unknown-none-elf` target selected by
`.cargo/config.toml`; the image row uses the host `espflash` converter.
Foundation revision `8e9d2a9cd72c356a96c2ead37d9311ed4a56e774` was clean.

| Gate | Command | Result |
|------|---------|--------|
| Supported host packages | `cargo test -p intercom-core -p intercom-adapter-http -p intercom-adapter-ota -p intercom-pairing-tool` | 114 passed, 0 failed |
| Firmware host library | `cargo test -p intercom-firmware --lib` | 9 passed, 0 failed |
| Partitioned KV tests | `cargo test -p intercom-firmware --test partitioned_kv_tests` | 8 passed, 0 failed |
| Strict core Clippy | `cargo clippy -p intercom-core --all-targets -- -D warnings` | passed |
| Scoped formatting | `rustfmt --check` over changed Rust files | passed |
| MCU release build | `cargo build -p intercom-firmware --release` (riscv32imac) | passed |
| OTA image size | host `espflash save-image --chip esp32c6 --flash-size 8mb` | 2,212,800 / 3,473,408 bytes (63.71%), fits |

The repository-wide `cargo fmt --all -- --check` baseline still reports
pre-existing differences in unrelated consumer and external Matter files. That
baseline is recorded as failing; only changed Rust files were formatted, and no
unrelated repositories were reformatted.

Strict core Clippy initially exposed three pre-existing lints unrelated to this
migration: a missing `Default` implementation for `AccessStateMachine` and for
a diagnostics test mock, plus a manual range check in the commissioning policy.
They were fixed in a separate maintenance commit so the required gate passes.

These are host and cross-compilation results only. No physical serial device,
firmware flash, OTA upload, private key, or hardware behavior was exercised.

## Matter identity and runtime wrapper slice

The follow-on Matter slice moves the remaining protocol-specific policy to
`edf-matter` while keeping product composition in Intercom:

- **Identity:** `intercom-adapter-matter` applies the `IC` serial prefix and
  `intercom-bridge` hostname prefix through
  `edf_matter::identity::derive_device_identity`. `MatterController` stores one
  static `DeviceIdentity` and uses it for the commissioning passcode,
  discriminator, serial number, and DHCP hostname. The duplicate
  `intercom-core::domain::commissioning` module and the adapter's serial/hostname
  formatters were removed. Both the device and `intercom-pairing-tool` now use
  the same foundation derivation.
- **Readiness:** `ClusterReadinessBarrier` wraps the foundation required-mask
  barrier for Operational publication, while retaining the Intercom Matter
  pointer and ecosystem endpoint reporting.
- **Storage:** `intercom_firmware::reset::PartitionedKvBlobStore` is now a
  compatibility alias for `edf_matter::store::PartitionedWifiStore`. The
  existing eight partition-routing integration tests pass unchanged.
- **Connection:** `ConnectionMonitor` uses foundation `FabricPolicy`,
  `has_active_controller_fabric`, and `ConnectionState` for the Apple Home Hub
  exclusion, network transitions, readiness reset, and decommission reset. The
  Intercom `UserTask` wrapper and startup orphan-only-fabric check remain.
- **MAC formats:** host tests cover colon, hyphen, and compact MAC input and
  compare the CLI credentials with the shared foundation identity.

Verification for this slice:

| Check | Command | Result |
|---|---|---|
| Host tests | `cargo test` for core, HTTP, OTA, pairing tool, firmware lib, and partitioned KV tests | 131 passed |
| Strict core Clippy | `cargo clippy -p intercom-core --all-targets -- -D warnings` | passed |
| MCU release build | `cargo build -p intercom-firmware --release` (riscv32imac) | passed |
| OTA image size | host `espflash save-image --chip esp32c6 --flash-size 8mb` | 2,211,472 / 3,473,408 bytes (63.67%), fits |
| Changed-file formatting | `rustfmt --check` on modified Rust files | passed |

A broader `-D warnings` Clippy run over the firmware remains blocked by a
pre-existing `unwrap_or_default` lint in `crates/intercom-firmware/build.rs`.
The strict core gate and the pairing-tool gate both pass.

The two adapter identity reference tests are `#[cfg(test)]` unit tests in the
MCU-only Matter crate, so they are not part of the host test total. The same
shared identity path, including the Intercom hostname, is host-covered through
`intercom-pairing-tool`. No physical device, flash, OTA upload, or private key
was used.

## GPIO product wrapper slice

The GPIO slice consumes `edf-gpio` through the workspace path dependency and
removes the remaining duplicated hardware implementations from
`intercom-adapter-gpio`:

- **Flash:** `intercom_adapter_gpio::flash` is now a compatibility re-export of
  `edf_gpio::flash::{EspFlash, EspFlashError}`. The local SPI flash
  implementation and its direct `embedded-storage` dependencies were removed;
  firmware import paths are unchanged.
- **Generic LED blink:** `edf_gpio::led::GpioLedDriver::blink(on_ms, off_ms,
  count)` is now public. It accepts an arbitrary finite (`Some(n)`) or
  continuous (`None`) pattern and only signals the existing indicator command
  loop, so it carries no Intercom concepts and is safe to reuse for any product.
- **Lifecycle LED:** `intercom_adapter_gpio::led::GpioLedDriver` wraps the
  foundation driver and continues to run the shared lifecycle state machine.
  Intercom ring and unlock feedback is layered on through `IntercomReporter`
  by issuing finite blinks with the existing product constants
  (`RING_BLINK_MS`/`RING_BLINK_COUNT` and
  `UNLOCK_BLINK_MS`/`UNLOCK_BLINK_COUNT`).
- **BOOT button:** `intercom_adapter_gpio::boot_button::GpioBootButtonDriver`
  wraps the foundation reset driver. The 5000 ms ecosystem and 10000 ms factory
  hold thresholds, 30 ms debounce, 100 ms polling, and hold feedback stages are
  preserved.
- **Configuration:** `intercom_adapter_gpio::config` now owns an explicit
  `IntercomLedProfile` via `edf_gpio::led_profile!` and the shared lifecycle
  LED patterns; only the Intercom ring and door constants remain locally
  defined. The GPIO config no longer re-exports shared LED/reset/lifecycle
  constants, and Intercom applies its own 5 s ecosystem / 10 s factory hold
  durations at runtime.
- **Ring and door:** both drivers remain product-owned and their behavior is
  unchanged.

One source-level API delta is intentional: `GpioBootButtonConfig` is now an
opaque wrapper around the foundation config. `new` and `with_indicator` remain,
and every in-repository call site is unchanged, but the previous public `pin`,
`indicator`, `ecosystem_hold_ms`, and `factory_hold_ms` fields are no longer
exposed.

Verification for this slice:

| Check | Command | Result |
|---|---|---|
| Foundation `edf-gpio` tests | `cargo test -p edf-gpio` (x86_64-apple-darwin) | 11 unit + 4 integration = 15 passed |
| GPIO adapter build | `cargo check -p intercom-adapter-gpio` (riscv32imac) | passed |
| Firmware build | `cargo check -p intercom-firmware` (riscv32imac) | passed |
| Intercom host tests | `cargo test` for core, HTTP, OTA, pairing tool, firmware lib, and partitioned KV tests | 131 passed |
| Strict core Clippy | `cargo clippy -p intercom-core --all-targets -- -D warnings` | passed |
| MCU release build | `cargo build -p intercom-firmware --release` (riscv32imac) | passed |
| OTA image size | host `espflash save-image --chip esp32c6 --flash-size 8mb` | 2,212,464 / 3,473,408 bytes (63.70%), fits |
| Changed-file formatting | `rustfmt --check` on modified Rust files | passed |

The GPIO adapter remains MCU-only, so wrapper behavior is covered by the
foundation integration test plus the RISC-V compile gate rather than a macOS
host test. This is a host and cross-compilation verification only: no physical
device, firmware flash, OTA upload, private key, or hardware behavior was
exercised.

## Logger and dashboard integration (latest verification)

The logger and dashboard migrations are now implemented in the Intercom
consumer. The logger composes `edf-logger` with Intercom-owned uptime, UART
formatting, severity colors, filters, and HTTP callback. The dashboard composes
`DashboardShell` with local door/chime controls and its consumer-owned
supplemental panel. OTA, key-generation, monitor, QR, and `cargo fe-*` wrappers
remain consumer-owned and unchanged, per the approved option 2.
An inline firmware test now exercises the public buffered-log callback against
the HTTP adapter's 4 KiB read window, including retention of the newest bytes
after the 6 KiB logger ring wraps.

The Tailwind `@source` path now scans the sibling Foundation package correctly.
A focused generated-asset test failed before the correction because the
embedded style block lacked `.min-h-screen` and `.max-w-4xl`, then passed after
rebuilding the single-file dashboard. The Intercom asset is 152,542 bytes; its
35,590-byte gzip payload decompresses byte-for-byte to the asset and is embedded
in the release ELF.

| Gate | Command/result | Outcome |
|------|---------------|---------|
| Host packages | `cargo test -p intercom-core -p intercom-adapter-http -p intercom-adapter-ota -p intercom-pairing-tool --target x86_64-apple-darwin --locked --offline` | 89 passed |
| Firmware host library | `cargo test -p intercom-firmware --lib --target x86_64-apple-darwin --locked --offline` | 6 passed |
| Partitioned KVS | `cargo test -p intercom-firmware --test partitioned_kv_tests --target x86_64-apple-darwin --locked --offline` | 8 passed |
| Strict core Clippy | `cargo clippy -p intercom-core --all-targets --target x86_64-apple-darwin --locked --offline -- -D warnings` | passed |
| Frontend | TypeScript, Biome, generated-asset Bun test, production Vite build | passed; test 1/1 |
| Firmware release | `cargo build -p intercom-firmware --release --target riscv32imac-unknown-none-elf --locked --offline` | passed |
| OTA slot image | `espflash save-image --chip esp32c6 --flash-size 8mb` | 2,224,416 / 3,473,408 bytes (64.04%), fits |
| Formatting / scope | changed Rust files with `rustfmt --check`; `git diff --check`; scripts and `.cargo` unchanged | passed |

The canonical frontend build wrapper attempted to rebuild Foundation and was
blocked by the sandbox's read-only Foundation path (`EPERM` creating
`node_modules/.vite-temp`). The consumer TypeScript, lint, test, and Vite build
were run directly against the already-built Foundation `dist`; no Foundation
files were changed. These are host and cross-compilation results only. No
browser interaction, hardware, serial device, private key, device network,
flash, or OTA upload was exercised.

## Remaining migration scope

The local foundation migrations for the current Intercom and HVAC consumers are
complete through logger and dashboard composition. Follow-on scope is limited
to optional host-script sharing, which remains deferred because the consumers'
existing script behavior differs:

1. OTA driver, OTA port, and HTTP error mapping together. *(done)*
2. Diagnostics and HTTP payload integration. *(done)*
3. Dashboard composition. *(done; shared CSS rules are verified in the generated asset)*
4. Host script wrappers. *(deferred by user choice; scripts remain consumer-owned)*

Publishing, remote creation, submodules, and `.gitmodules` changes remain out of
scope. The Intercom consumer changes remain uncommitted in the existing dirty
checkout so the concurrent work already there is preserved.
