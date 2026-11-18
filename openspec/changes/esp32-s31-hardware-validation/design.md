# Design

## Context

See `proposal.md` for the reason this validation is separate from the firmware migration. The migration already provides host tests, target compile checks, a five-packet jitter buffer, PSRAM allocation, and media benchmark logging. A physical S31 board is needed to validate Core 1 timing, sustained media behavior, real memory availability, and S31 OTA behavior.

## Goals / Non-Goals

**Goals:**

- Run the existing performance and stability checks on an S31 board and preserve the serial, `/api/status`, and OTA evidence.
- Use the firmware's existing WebRTC benchmark output to assess encoding latency, heap minimum, and audio cadence.
- Validate S31 signed update and rollback behavior using an S31-compatible image.

**Non-Goals:**

- Change the S31 platform architecture or tune memory allocation without measurements from the target board.
- Validate or redesign the PCB.
- Treat the C6 DevKit's OTA-disabled 4 MB configuration as evidence for S31 OTA behavior.

## Decisions

- Use the S31 release image on the physical board with the board's documented flash and PSRAM configuration. Record the firmware commit, board revision, and flash size with the results.
- Start the serial monitor without resetting the board. Record the Opus timing, audio cadence, stack, and heap benchmark lines before and after the sustained stream.
- Run `python3 scripts/test_matter_tcp.py` before opening the Home Assistant WebRTC Listen stream. Keep the stream active for more than 180 seconds, then verify it returns to idle after ending the stream.
- Check `/api/status` before and after streaming. Confirm S31 reports PSRAM and record internal and PSRAM free memory.
- Supply OTA with a pre-generated S31 `.bin` image. The current ELF conversion path in `scripts/ota.py` is C6-specific, so do not pass an S31 ELF to it. Preserve a known-good slot before exercising rollback.

## Risks / Trade-offs

- **S31 image packaging may need an S31-capable `espflash` version.** Verify the chip and flash-size arguments against the board before generating the OTA image.
- **A rollback trial can leave the board without a bootable new slot.** Keep a known-good image and USB recovery path available before beginning.
- **A successful compile does not establish S31 timing or PSRAM performance.** Accept results only from serial logs and the live status API on the physical target.
