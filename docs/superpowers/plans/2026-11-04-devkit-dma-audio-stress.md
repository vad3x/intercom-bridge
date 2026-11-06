# DevKit DMA-Timed Audio Stress Simulation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the DevKit test tone emulate real-time I2S DMA capture so on-device logs reveal whether WebRTC processing misses capture deadlines or overflows the RX software queue.

**Architecture:** Add a pure, host-testable capture schedule to `intercom-media`. Use it only in the DevKit tone task to backfill no more than the production DMA ring's four frames, record missed DMA frames, and feed generated frames through the existing RX channel. Reuse the shared drop window for RX queue failures; production capture timing remains unchanged.

**Tech Stack:** Rust `no_std`, Embassy time, `intercom-media`, ESP32-C6 DevKit firmware, `mise`.

## Global Constraints

- The simulator runs only under the `devkit` feature.
- One capture frame represents 20 ms at 16 kHz; the DMA ring holds four frames (80 ms).
- Keep the existing production I2S capture and WebRTC send behavior unchanged.
- Run Cargo and Rust commands through `mise exec -- ...`.
- A DevKit simulation cannot verify codec wiring or analog input.

---

### Task 1: Add the testable capture deadline model

**Files:**
- Modify: `crates/intercom-media/src/lib.rs`
- Test: `crates/intercom-media/src/lib.rs`

**Interfaces:**
- Produces `AudioCaptureSchedule::new(now_ms: u64)`, `delay_until_next_ms(now_ms: u64) -> u64`, and `take_due_frames(now_ms: u64) -> AudioCaptureBatch`.
- `AudioCaptureBatch` exposes `frames: u8` and `overruns: u64`.
- Exposes `AUDIO_CAPTURE_INTERVAL_MS: u64 = 20` and `AUDIO_DMA_BUFFER_FRAMES: u8 = 4` for the firmware integration.

- [ ] **Step 1: Add failing tests for deadline and backlog behavior**

```rust
#[test]
fn audio_capture_schedule_uses_absolute_20ms_deadlines() {
    // Arrange
    let mut schedule = AudioCaptureSchedule::new(0);

    // Act
    let early = schedule.take_due_frames(19);
    let first = schedule.take_due_frames(20);
    let late = schedule.take_due_frames(50);

    // Assert
    assert_eq!(early, AudioCaptureBatch { frames: 0, overruns: 0 });
    assert_eq!(schedule.delay_until_next_ms(50), 10);
    assert_eq!(first, AudioCaptureBatch { frames: 1, overruns: 0 });
    assert_eq!(late, AudioCaptureBatch { frames: 1, overruns: 0 });
}

#[test]
fn audio_capture_schedule_caps_backlog_at_dma_capacity_and_counts_overruns() {
    // Arrange
    let mut schedule = AudioCaptureSchedule::new(0);

    // Act
    let batch = schedule.take_due_frames(100);

    // Assert
    assert_eq!(
        batch,
        AudioCaptureBatch {
            frames: 4,
            overruns: 1,
        }
    );
    assert_eq!(schedule.delay_until_next_ms(100), 20);
}
```

- [ ] **Step 2: Run the focused tests and confirm the expected compile failure**

Run:

```bash
CARGO_TARGET_DIR=/private/tmp/matter-ssrc-target mise exec -- cargo test --locked -p intercom-media --lib --target x86_64-apple-darwin audio_capture_schedule -- --nocapture
```

Expected: compilation fails because `AudioCaptureSchedule` and `AudioCaptureBatch` do not exist yet.

- [ ] **Step 3: Implement the schedule and shared DMA capacity constant**

```rust
pub const AUDIO_CAPTURE_INTERVAL_MS: u64 = 20;
pub const AUDIO_DMA_BUFFER_FRAMES: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioCaptureBatch {
    pub frames: u8,
    pub overruns: u64,
}

pub struct AudioCaptureSchedule {
    next_frame_ms: u64,
}

impl AudioCaptureSchedule {
    pub const fn new(now_ms: u64) -> Self {
        Self {
            next_frame_ms: now_ms + AUDIO_CAPTURE_INTERVAL_MS,
        }
    }

    pub fn delay_until_next_ms(&self, now_ms: u64) -> u64 {
        self.next_frame_ms.saturating_sub(now_ms)
    }

    pub fn take_due_frames(&mut self, now_ms: u64) -> AudioCaptureBatch {
        if now_ms < self.next_frame_ms {
            return AudioCaptureBatch {
                frames: 0,
                overruns: 0,
            };
        }

        let elapsed_intervals = now_ms.saturating_sub(self.next_frame_ms)
            / AUDIO_CAPTURE_INTERVAL_MS
            + 1;
        self.next_frame_ms = self.next_frame_ms.saturating_add(
            elapsed_intervals.saturating_mul(AUDIO_CAPTURE_INTERVAL_MS),
        );
        let capacity = u64::from(AUDIO_DMA_BUFFER_FRAMES);
        AudioCaptureBatch {
            frames: elapsed_intervals.min(capacity) as u8,
            overruns: elapsed_intervals.saturating_sub(capacity),
        }
    }
}
```

- [ ] **Step 4: Rerun the focused tests and the full media test suite**

Run:

```bash
CARGO_TARGET_DIR=/private/tmp/matter-ssrc-target mise exec -- cargo test --locked -p intercom-media --lib --target x86_64-apple-darwin audio_capture_schedule -- --nocapture
CARGO_TARGET_DIR=/private/tmp/matter-ssrc-target mise exec -- cargo test --locked -p intercom-media --lib --target x86_64-apple-darwin
```

Expected: both schedule tests pass, then the full `intercom-media` suite passes.

- [ ] **Step 5: Commit the schedule and tests**

```bash
git add crates/intercom-media/src/lib.rs
git commit -m "feat(media): model DMA-paced audio capture"
```

### Task 2: Integrate the schedule into DevKit and verify the firmware

**Files:**
- Modify: `crates/intercom-adapter-audio/src/i2s_pipeline.rs`
- Modify: `crates/intercom-firmware/src/main.rs`
- Test: DevKit runtime logs and production release build

**Interfaces:**
- Consumes `AudioCaptureSchedule`, `AudioCaptureBatch`, `AudioFrameDropWindow`, `AUDIO_CAPTURE_INTERVAL_MS`, and `AUDIO_DMA_BUFFER_FRAMES` from `intercom-media`.
- DevKit logs one line per 50 generated frames with `simulated_dma_overruns` and `queue_drops`.
- Production I2S DMA capacity uses the shared four-frame constant without changing its timing.

- [ ] **Step 1: Use the shared DMA capacity constant in the production buffer size**

```rust
const DMA_STREAM_BYTES: usize =
    DMA_FRAME_BYTES * intercom_media::AUDIO_DMA_BUFFER_FRAMES as usize;
```

- [ ] **Step 2: Replace the DevKit relative-delay producer with absolute-deadline batches**

```rust
let mut schedule = intercom_media::AudioCaptureSchedule::new(
    embassy_time::Instant::now().as_millis(),
);
let mut drop_window = intercom_media::AudioFrameDropWindow::new();
let mut simulated_dma_overruns = 0u64;

loop {
    let now_ms = embassy_time::Instant::now().as_millis();
    embassy_time::Timer::after_millis(schedule.delay_until_next_ms(now_ms)).await;
    let batch = schedule.take_due_frames(embassy_time::Instant::now().as_millis());
    simulated_dma_overruns = simulated_dma_overruns.saturating_add(batch.overruns);

    for _ in 0..batch.frames {
        let queue_dropped = RX_AUDIO_CHANNEL
            .try_send(tone.next_frame())
            .is_err();
        if let Some(queue_drops) = drop_window.record(queue_dropped) {
            info!(
                "DevKit audio capture frames=50 simulated_dma_overruns={} queue_drops={}",
                simulated_dma_overruns,
                queue_drops,
            );
            simulated_dma_overruns = 0;
        }
    }
}
```

- [ ] **Step 3: Build the DevKit firmware**

Run:

```bash
CARGO_TARGET_DIR=/private/tmp/matter-ssrc-devkit-target mise exec -- cargo build-devkit
```

Expected: optimized DevKit firmware build succeeds.

- [ ] **Step 4: Flash and inspect live counters during HA Listen**

Check that the board port is free before flashing:

```bash
lsof -nP /dev/cu.usbmodem14201
```

Run:

```bash
espflash flash /private/tmp/matter-ssrc-devkit-target/riscv32imac-unknown-none-elf/release/intercom-bridge --port /dev/cu.usbmodem14201 --partition-table partitions-4mb.csv
```

Wait for `/api/status` to report `Operational`, start HA Listen, and inspect `/api/logs` for `DevKit audio capture frames=50 simulated_dma_overruns=... queue_drops=...` and `WebRTC audio cadence` entries. Confirm `/api/status` reports at least 32,768 free heap bytes while Listen is active.

Expected: both counters are present while active. Record whether either value is nonzero; nonzero values indicate the corresponding simulated capture or software queue overrun.

- [ ] **Step 5: Confirm the production build still compiles**

Run:

```bash
CARGO_TARGET_DIR=/private/tmp/matter-ssrc-devkit-target mise exec -- cargo build -p intercom-firmware --release
```

Expected: production firmware build succeeds with the unchanged real I2S path.

- [ ] **Step 6: Commit the DevKit integration**

```bash
git add crates/intercom-adapter-audio/src/i2s_pipeline.rs crates/intercom-firmware/src/main.rs
git commit -m "test(devkit): emulate DMA-paced audio capture"
```
