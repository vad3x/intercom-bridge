# Tasks

## 1. Core 1 Opus Performance

- [ ] 1.1 [RED] Add an in-source host test for tracking the maximum S31 Opus encode duration, verifying failure with: `mise exec -- cargo test -p intercom-firmware --lib --target x86_64-apple-darwin platform::synthetic_audio::tests::tracks_max_encode_duration`
- [ ] 1.2 [GREEN] Instrument the S31 Core 1 audio worker to report maximum Opus encode duration, verifying the test passes with: `mise exec -- cargo test -p intercom-firmware --lib --target x86_64-apple-darwin platform::synthetic_audio::tests::tracks_max_encode_duration`
- [ ] 1.3 Benchmark the S31 Core 1 pipeline at 320 MHz, confirm encoding stays below 5 ms per frame with no stack overflow, and record the serial output using: `python3 scripts/monitor.py --no-reset --port /dev/cu.usbmodem14201`

## 2. Sustained WebRTC Listen

- [ ] 2.1 Run `python3 scripts/test_matter_tcp.py` before opening Home Assistant's Listen stream, keep the WebRTC stream active for more than 180 seconds, verify it returns to idle after ending the stream, and record serial logs plus `/api/status` memory values. Require at least 32,768 bytes of minimum free heap after the call.

## 3. S31 Signed OTA and Rollback

- [ ] 3.1 Prepare an S31-compatible `.bin` image, then run `python3 scripts/ota.py /private/tmp/intercom-s31.bin` to sign and upload it. Exercise rollback with a signed test image that fails self-validation, confirm the previous known-good slot boots, and preserve serial and status API evidence.
