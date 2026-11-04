# DevKit DMA-Timed Audio Stress Simulation

## Goal

Use the connected ESP32-C6 DevKit to measure whether the WebRTC media loop can keep up with a real-time 16 kHz capture source when no codec-equipped board is available.

## Scope

The simulator is compiled only for the `devkit` feature. The production I2S capture path and its new queue-drop diagnostics remain unchanged.

## Design

The DevKit tone producer will use an absolute 20 ms capture deadline rather than waiting 20 ms after each software iteration. If the executor resumes late, the simulator will calculate how many capture frames became due. It will model the production I2S DMA ring's four-frame, 80 ms capacity: emit up to the latest four pending frames and count older missed frames as simulated DMA overruns. Each emitted frame is sent through the existing `RX_AUDIO_CHANNEL`; the shared frame-drop window records queue-full failures.

Every 50 emitted frames, DevKit logs will report the number of simulated DMA overruns and RX queue drops. The counters distinguish missed capture time from software queue overflow.

## Tests and verification

- Host unit tests cover the 20 ms deadline, late wake-up calculation, four-frame backlog cap, and overrun count.
- `mise exec -- cargo build-devkit` verifies the DevKit integration.
- Flash the DevKit and run HA Listen. Inspect cadence and counter logs while the stream is active.
- `mise exec -- cargo build -p intercom-firmware --release` confirms the production build remains valid.

## Limits

This test measures scheduler and media-path pressure on the DevKit. It cannot verify AIC3204 wiring, analog input levels, or physical I2S signal integrity. A codec-equipped board is still required for those checks.
