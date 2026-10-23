# Local foundation pairing integration

The host pairing tool consumes `edf-pairing-tool` from a sibling checkout of
`embedded-device-foundation`. The validated foundation revision is `1011623`.
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
Cargo path is relative to the Intercom repository, so an equivalent sibling
layout is required on every build machine.

This is an unpinned local path dependency. Cargo.lock records the local
package versions and dependency graph, but it does not fix the foundation Git
revision. Check out `1011623` when reproducing these host results and review
future foundation changes before updating this integration.

The Intercom CLI retains its argument parsing, serial discovery, and terminal
QR rendering. Its generated credentials and JSON formatting come from the
foundation crate. The validation gate for this slice is host tests, formatting,
and Clippy for the pairing tool, plus the supported Intercom host packages.
Firmware dependencies were not changed. This validation makes no claim about
release firmware builds, physical hardware, publication, or submodules.
