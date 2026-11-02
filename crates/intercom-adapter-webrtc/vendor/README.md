# C source provenance

- `opus/`: Opus 1.5.2, from the official release archive
  https://downloads.xiph.org/releases/opus/opus-1.5.2.tar.gz
  SHA-256: `65c1d2f78b9f2fb20082c38cbe47c951ad5839345876e46941612ee87f9a7ce1`.
  Source/header trees and upstream make source manifests are retained; test trees
  are omitted. License: BSD-style, see `opus/COPYING` and individual file notices.
- `mbedtls/`: Mbed TLS 3.6.5 from `mbedtls-rs-sys` 0.3.1's bundled upstream
  `mbedtls` tree in the Cargo registry. `include/mbedtls/build_info.h` identifies
  version 3.6.5 (`0x03060500`). The include/library trees, ChangeLog and dual
  Apache-2.0 OR GPL-2.0-or-later LICENSE are retained.

Vendor source files are unmodified. Build configuration is in `../csrc` and
`../build.rs`.
