# Local foundation integration

The host pairing tool and `intercom-core` consume crates from a sibling checkout
of `embedded-device-foundation`. The validated foundation revision is `1011623`.
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
revision. Check out `1011623` when reproducing these results and review future
foundation changes before updating this integration.

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
Foundation revision `1011623` was clean.

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

## Remaining migration scope

Stage A does not complete the foundation integration. Follow-on slices remain,
each requiring an audit and plan against the actual foundation interfaces:

1. GPIO product wrapper.
2. OTA driver, OTA port, and HTTP error mapping together.
3. Matter identity, readiness, storage, and connection wrappers, including
   converging the device-side commissioning identity in `intercom-core` with the
   foundation/CLI pairing identity and cross-checking them across multiple MAC
   formats.
4. Diagnostics and HTTP payload integration.
5. Dashboard composition.
6. Host script wrappers.

Finish Intercom before applying the validated approach to HVAC. Publishing,
remote creation, submodules, and `.gitmodules` changes remain out of scope.
