# Tasks: Operational Matter-over-TCP Review Fixes

## 1. CI Fixes, Feature Cleanup & Commit Hygiene (rs-matter)

- [ ] 1.1 Split the `rs-matter-transport/src/network/tcp.rs` -> `tcp/std.rs` move into a dedicated first commit and verify `git show --stat` shows a pure file move (#20)
- [ ] 1.2 Add `MaybeSend` bound to `F` on `MatterStack::run_transport_oper` (`rs-matter-stack/src/lib.rs:685`) and verify `cargo check -p rs-matter-stack --features send-sync,std` compiles cleanly (#1)
- [ ] 1.3 Add `#[cfg_attr(miri, ignore)]` to `real_os_socket_*` and timing-sensitive `idle_peer_releases_its_socket_and_a_new_peer_can_connect` in `rs-matter-transport`, store wakers in `Poll::Pending` test mocks, and verify `cargo miri test -p rs-matter-transport` passes (#2, #19)
- [ ] 1.4 Remove `operational-tcp` and `stack-tcp` Cargo features from `rs-matter`, `rs-matter-stack`, and `rs-matter-dm` (restoring `pub const fn new` on `AudioAvStreamHandler` and `CameraAvStreamHandler`), replace `ErrorCode::NoSpace` with `ErrorCode::ResourceExhausted` in TX queue/`FullListener` tests, fix `check_frame_slice` docs, and verify `cargo clippy --no-deps --all-targets -- -Dwarnings` passes (#6, #18, #25)
- [ ] 1.5 Enable operational TCP tests in `.github/workflows/ci.yml` and verify `cargo test -p rs-matter-stack --test drivers` runs and passes (#3)

## 2. Large Buffer Sizing & Session Parameters (rs-matter)

- [ ] 2.1 Add `large-buffers-4096`..`large-buffers-1048576` Cargo features (`large-buffers` aliasing `large-buffers-65536`, largest wins) deriving transport RX/TX, `IM_BUFFER_SIZE`, and `MAX_TCP_MESSAGE_SIZE` from one definition, and verify feature-matrix compilation across UDP-only, 4096, and 65536 profiles (#4)
- [ ] 2.2 Add const-asserted `N <= MAX_RX_BUF_SIZE` on `TcpNetwork`, advertise `SUPPORTED_TRANSPORTS` and `MAX_TCP_MESSAGE_SIZE` in `rs-matter-sc::local_session_parameters`, send `MESSAGE_TOO_LARGE` before closing oversized inbound frames, and verify with unit tests in `rs-matter-sc` and `rs-matter-transport` (#4)

## 3. Unified TcpNetwork, Async-fn Socket Traits & Static Placement (rs-matter)

- [ ] 3.1 Replace `NetStack::Tcp` with `NetTcpStack` and default `NetStack::tcp(&self) -> Option<impl NetTcpStack + '_>` returning `None::<NoTcp>`, and verify downstream-style `NetStack` impls without `tcp()` compile unchanged (#5)
- [ ] 3.2 Define cancel-safe async-fn socket traits (`accept`, `read`, `write`, `close`, `connect`, `set_keepalive`), update `tcp/std.rs` (including `send_timeout` connection teardown), document `NetworkReceive::wait_available` as cancel-safe, and verify `rs-matter-transport` std tests pass (#8, #9, #12, #13)
- [ ] 3.3 Unify `TcpServer` + client/server into `TcpNetwork` (`rs-matter-transport/src/network/tcp.rs`) using `utils::storage::Vec<u8, N>`, `init()` / `init_array_from_fn` static placement, direct reads without a 256-byte bounce buffer or blocking mutex held across `.await`, and log-and-retry on accept errors; verify with unit tests covering static `init()`, direct reads, and accept error recovery (#9, #10, #11, #15, #16)

## 4. Multi-Slot, Keep-Alive, Backpressure, Session Eviction & mDNS (rs-matter)

- [ ] 4.1 Support `SLOTS` const generic on `TcpNetwork` with unauthenticated/longest-idle eviction, and verify multi-peer concurrency and slot eviction with unit tests (#14)
- [ ] 4.2 Replace the 60s idle timeout with TCP keep-alive and use bounded backpressure with `ErrorCode::ResourceExhausted` instead of closing on a 2-frame full TX queue; verify quiet connection persistence and backpressure tests pass (#17, #23, #24)
- [ ] 4.3 Propagate TCP connection-closed notifications to `rs-matter-transport` so active exchanges close and TCP-bound sessions are evicted/marked immediately, and dynamically set mDNS `T=4` (server-only) or `T=6` (client+server) only after `bind()` succeeds; verify with `rs-matter-stack` integration tests (#7, #21, #22)

## 5. Downstream Integration & Hardware Verification (rs-matter-embassy & intercom-matter)

- [ ] 5.1 Implement `NetTcpStack` and async-fn socket traits over `embassy-net` 0.9 (`smoltcp`) in a companion `rs-matter-embassy` PR and verify it compiles for `riscv32imac-unknown-none-elf` (#8)
- [ ] 5.2 Wire `intercom-matter` to static TCP buffers, enable `large-buffers-4096` + `max-im-buffers-4`, and verify `cargo test` (host), `cargo clippy --no-deps --all-targets -- -Dwarnings`, and `cargo build -p intercom-firmware --release` pass
- [ ] 5.3 Flash via USB (no OTA) when hardware is attached and verify `scripts/test_matter_tcp.py`, multi-KB SDP WebRTC signaling, and sustained Listen (`min_free_heap_bytes >= 32768`)
