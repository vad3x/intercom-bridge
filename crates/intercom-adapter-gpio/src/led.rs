//! Smart Intercom - GPIO LED Indicator Driver
//!
//! Signal-based LED driver: sync methods signal commands, `run` loop
//! drives GPIO. Replaces the C++ GpioLedDriver (output + FreeRTOS blink timer).

use core::cell::RefCell;

use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use esp_hal::gpio::{Level, Output, OutputConfig};
use intercom_core::ports::IndicatorDriver;
use log::info;

/// GPIO-based LED indicator driver (active-low red LED).
///
/// LED is ON when GPIO is LOW, OFF when GPIO is HIGH.
/// Sync methods signal commands; `run` loop handles GPIO + timing.
pub struct GpioLedDriver {
    pin: RefCell<Output<'static>>,
    signal: Signal<CriticalSectionRawMutex, IndicatorCommand>,
}

impl GpioLedDriver {
    /// Create a new LED driver with the given output pin.
    pub fn new(pin: impl esp_hal::gpio::OutputPin + 'static) -> Self {
        info!("created (active-low)");

        Self {
            pin: RefCell::new(Output::new(pin, Level::High, OutputConfig::default())), // Start OFF
            signal: Signal::new(),
        }
    }

    /// Execute a command. Returns the next pending command if interrupted.
    async fn next(&self, cmd: IndicatorCommand) -> Option<IndicatorCommand> {
        match cmd {
            IndicatorCommand::On => {
                self.led_on();
                None
            }
            IndicatorCommand::Blink {
                on_ms,
                off_ms,
                count: None,
            } => {
                // Continuous blink until interrupted by a new command
                loop {
                    if let Some(next) = self.blink_pattern(1, on_ms, off_ms).await {
                        return Some(next);
                    }
                }
            }
            IndicatorCommand::Blink {
                on_ms,
                off_ms,
                count: Some(n),
            } => {
                // Finite blink — return to steady on when done
                let interrupted = self.blink_pattern(n, on_ms, off_ms).await;
                if interrupted.is_none() {
                    self.led_on();
                }
                interrupted
            }
            IndicatorCommand::Burst {
                on_ms,
                off_ms,
                burst_count,
                pause_ms,
            } => {
                // Continuous burst: N quick blinks + long pause, repeating
                loop {
                    // Do the quick blinks
                    if let Some(next) = self.blink_pattern(burst_count, on_ms, off_ms).await {
                        return Some(next);
                    }
                    // Long pause (LED stays off after blink_pattern)
                    if let Some(next) = self.interruptible_wait(pause_ms).await {
                        return Some(next);
                    }
                }
            }
        }
    }

    /// Blink the LED `count` times with given ON/OFF durations.
    /// Returns `Some(cmd)` if interrupted by a new command, `None` if completed.
    async fn blink_pattern(&self, count: u8, on_ms: u64, off_ms: u64) -> Option<IndicatorCommand> {
        for _ in 0..count {
            self.led_on();
            match select(
                Timer::after(Duration::from_millis(on_ms)),
                self.signal.wait(),
            )
            .await
            {
                Either::First(()) => {}
                Either::Second(cmd) => return Some(cmd),
            }

            self.led_off();
            match select(
                Timer::after(Duration::from_millis(off_ms)),
                self.signal.wait(),
            )
            .await
            {
                Either::First(()) => {}
                Either::Second(cmd) => return Some(cmd),
            }
        }
        None
    }

    /// Wait for `ms` milliseconds, interruptible by a new command.
    /// Returns `Some(cmd)` if interrupted, `None` if the wait completed.
    async fn interruptible_wait(&self, ms: u64) -> Option<IndicatorCommand> {
        match select(Timer::after(Duration::from_millis(ms)), self.signal.wait()).await {
            Either::First(()) => None,
            Either::Second(cmd) => Some(cmd),
        }
    }

    /// Turn LED on (active-low: GPIO LOW)
    fn led_on(&self) {
        self.pin.borrow_mut().set_low();
    }

    /// Turn LED off (active-low: GPIO HIGH)
    fn led_off(&self) {
        self.pin.borrow_mut().set_high();
    }
}

impl IndicatorDriver for GpioLedDriver {
    fn solid(&self) {
        self.signal.signal(IndicatorCommand::On);
    }

    fn blink(&self, on_ms: u64, off_ms: u64, count: Option<u8>) {
        self.signal.signal(IndicatorCommand::Blink {
            on_ms,
            off_ms,
            count,
        });
    }

    fn burst(&self, on_ms: u64, off_ms: u64, burst_count: u8, pause_ms: u64) {
        self.signal.signal(IndicatorCommand::Burst {
            on_ms,
            off_ms,
            burst_count,
            pause_ms,
        });
    }

    async fn run(&self) {
        // Wait for commands and execute them
        let mut cmd = self.signal.wait().await;

        loop {
            // Execute the command; if interrupted, handle the new command immediately
            match self.next(cmd).await {
                Some(next) => cmd = next,
                None => cmd = self.signal.wait().await,
            }
        }
    }
}

/// Commands the actor sends to the driver's `run` loop via `Signal`.
#[derive(Clone, Copy, Debug)]
enum IndicatorCommand {
    /// Steady on.
    On,
    /// Blink with given on/off durations (ms).
    ///
    /// - `count: None` — blink continuously until the next command.
    /// - `count: Some(n)` — blink `n` times, then return to steady on.
    Blink {
        on_ms: u64,
        off_ms: u64,
        count: Option<u8>,
    },
    /// Burst: `burst_count` quick blinks, then a long pause, repeating.
    ///
    /// Creates an irregular rhythm (e.g. double-flash for error state).
    Burst {
        on_ms: u64,
        off_ms: u64,
        burst_count: u8,
        pause_ms: u64,
    },
}
