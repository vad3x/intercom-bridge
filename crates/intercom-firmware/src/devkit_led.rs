//! DevKit WS2812 RGB LED Indicator Driver
//!
//! Controls the onboard addressable WS2812B RGB LED on GPIO 8 of the
//! ESP32-C6-DevKitM-1 via the RMT peripheral.

use core::cell::RefCell;
use edf_core::events::{SystemState, SystemStateReceiver};
use edf_gpio::led::HoldFeedback;
use embassy_futures::join::join;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use intercom_core::ports::reporter::{IntercomReporter, LockState};
#[allow(unused_imports)]
use log::info;

#[cfg(all(target_arch = "riscv32", feature = "led-smartled"))]
use esp_hal::rmt::Rmt;
#[cfg(all(target_arch = "riscv32", feature = "led-smartled"))]
use esp_hal::time::Rate;
#[cfg(all(target_arch = "riscv32", feature = "led-smartled"))]
use esp_hal_smartled::{buffer_size, color_order, RmtSmartLeds, WS2812B_TIMING};
#[cfg(all(target_arch = "riscv32", feature = "led-smartled"))]
use smart_leds::{SmartLedsWrite, RGB8};

#[cfg(not(all(target_arch = "riscv32", feature = "led-smartled")))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct RGB8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub const COLOR_OFF: RGB8 = RGB8 { r: 0, g: 0, b: 0 };
pub const COLOR_BOOTING: RGB8 = RGB8 { r: 30, g: 15, b: 0 };
pub const COLOR_COMMISSIONING: RGB8 = RGB8 { r: 0, g: 0, b: 40 };
pub const COLOR_WIFI_CONNECTING: RGB8 = RGB8 { r: 0, g: 30, b: 30 };
pub const COLOR_ECOSYSTEM_CONNECTING: RGB8 = RGB8 { r: 30, g: 0, b: 30 };
pub const COLOR_READY: RGB8 = RGB8 { r: 0, g: 8, b: 0 };
pub const COLOR_UPDATING: RGB8 = RGB8 { r: 40, g: 0, b: 40 };
pub const COLOR_ERROR: RGB8 = RGB8 { r: 50, g: 0, b: 0 };
pub const COLOR_RING: RGB8 = RGB8 { r: 60, g: 30, b: 0 };
pub const COLOR_UNLOCK: RGB8 = RGB8 { r: 0, g: 60, b: 0 };
pub const COLOR_HOLD_PRESS: RGB8 = RGB8 {
    r: 15,
    g: 15,
    b: 15,
};
pub const COLOR_HOLD_STAGE1: RGB8 = RGB8 { r: 50, g: 25, b: 0 };
pub const COLOR_HOLD_STAGE2: RGB8 = RGB8 { r: 60, g: 0, b: 0 };

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LedCommand {
    State(SystemState),
    Ring(bool),
    Unlock,
    HoldPress,
    HoldStage1,
    HoldStage2,
    Restore,
}

/// Addressable RGB LED driver for ESP32-C6-DevKitM-1.
pub struct DevKitLedDriver {
    #[cfg(all(target_arch = "riscv32", feature = "led-smartled"))]
    led: RefCell<
        RmtSmartLeds<
            'static,
            { buffer_size::<RGB8>(1) },
            esp_hal::Blocking,
            RGB8,
            color_order::Grb,
        >,
    >,
    #[cfg(not(all(target_arch = "riscv32", feature = "led-smartled")))]
    last_color: RefCell<RGB8>,
    signal: Signal<CriticalSectionRawMutex, LedCommand>,
    current_state: RefCell<Option<SystemState>>,
}

impl DevKitLedDriver {
    /// Create a new DevKit LED driver on RISC-V with RMT and GPIO8.
    #[cfg(all(target_arch = "riscv32", feature = "led-smartled"))]
    pub fn new(
        rmt_periph: esp_hal::peripherals::RMT<'static>,
        pin: esp_hal::peripherals::GPIO8<'static>,
    ) -> Self {
        info!("DevKitLedDriver: initializing WS2812 on GPIO 8 via RMT");
        let rmt = Rmt::new(rmt_periph, Rate::from_mhz(80)).expect("RMT init failed");
        let led = RmtSmartLeds::<{ buffer_size::<RGB8>(1) }, _, RGB8, color_order::Grb>::new(
            WS2812B_TIMING,
            rmt.channel0,
            pin,
            Rate::from_mhz(80),
        )
        .expect("RmtSmartLeds init failed");

        let driver = Self {
            led: RefCell::new(led),
            signal: Signal::new(),
            current_state: RefCell::new(None),
        };
        driver.write_color(COLOR_BOOTING);
        driver
    }

    /// Create a new DevKit LED driver on host or when smartled is disabled.
    #[cfg(not(all(target_arch = "riscv32", feature = "led-smartled")))]
    pub fn new() -> Self {
        Self {
            last_color: RefCell::new(COLOR_OFF),
            signal: Signal::new(),
            current_state: RefCell::new(None),
        }
    }

    fn write_color(&self, color: RGB8) {
        #[cfg(all(target_arch = "riscv32", feature = "led-smartled"))]
        {
            if let Ok(mut led) = self.led.try_borrow_mut() {
                let _ = led.write([color]);
            }
        }
        #[cfg(not(all(target_arch = "riscv32", feature = "led-smartled")))]
        {
            *self.last_color.borrow_mut() = color;
        }
    }

    #[cfg(not(all(target_arch = "riscv32", feature = "led-smartled")))]
    pub fn last_color(&self) -> RGB8 {
        *self.last_color.borrow()
    }

    /// Background async runner that observes lifecycle states and executes LED patterns.
    pub async fn run(&self, mut state_rx: SystemStateReceiver<'_>) {
        join(self.command_loop(), self.observe_state(&mut state_rx)).await;
    }

    async fn observe_state(&self, state_rx: &mut SystemStateReceiver<'_>) {
        loop {
            let state = state_rx.get().await;
            *self.current_state.borrow_mut() = Some(state);
            self.signal.signal(LedCommand::State(state));
            let _ = state_rx.changed().await;
        }
    }

    async fn command_loop(&self) {
        let mut active_state = SystemState::Booting;
        let mut ringing = false;
        let mut unlock_until: Option<embassy_time::Instant> = None;
        let mut hold_mode = false;

        loop {
            // Check for new commands or timeouts
            if self.signal.signaled() {
                match self.signal.wait().await {
                    LedCommand::State(s) => {
                        active_state = s;
                        hold_mode = false;
                    }
                    LedCommand::Ring(r) => {
                        ringing = r;
                    }
                    LedCommand::Unlock => {
                        unlock_until =
                            Some(embassy_time::Instant::now() + Duration::from_millis(3000));
                    }
                    LedCommand::HoldPress => {
                        hold_mode = true;
                        self.write_color(COLOR_HOLD_PRESS);
                    }
                    LedCommand::HoldStage1 => {
                        hold_mode = true;
                        self.write_color(COLOR_HOLD_STAGE1);
                    }
                    LedCommand::HoldStage2 => {
                        hold_mode = true;
                        self.write_color(COLOR_HOLD_STAGE2);
                    }
                    LedCommand::Restore => {
                        hold_mode = false;
                    }
                }
            }

            if hold_mode {
                Timer::after(Duration::from_millis(50)).await;
                continue;
            }

            let now = embassy_time::Instant::now();
            let is_unlocking = unlock_until.is_some_and(|until| now < until);

            if is_unlocking {
                self.write_color(COLOR_UNLOCK);
                Timer::after(Duration::from_millis(200)).await;
                self.write_color(COLOR_OFF);
                Timer::after(Duration::from_millis(200)).await;
                continue;
            }

            if ringing {
                self.write_color(COLOR_RING);
                Timer::after(Duration::from_millis(150)).await;
                self.write_color(COLOR_OFF);
                Timer::after(Duration::from_millis(150)).await;
                continue;
            }

            // Normal lifecycle pattern
            match active_state {
                SystemState::Booting => {
                    self.write_color(COLOR_BOOTING);
                    Timer::after(Duration::from_millis(500)).await;
                    self.write_color(COLOR_OFF);
                    Timer::after(Duration::from_millis(500)).await;
                }
                SystemState::Starting => {
                    self.write_color(COLOR_BOOTING);
                    Timer::after(Duration::from_millis(200)).await;
                    self.write_color(COLOR_OFF);
                    Timer::after(Duration::from_millis(200)).await;
                }
                SystemState::Commissioning => {
                    self.write_color(COLOR_COMMISSIONING);
                    Timer::after(Duration::from_millis(400)).await;
                    self.write_color(COLOR_OFF);
                    Timer::after(Duration::from_millis(400)).await;
                }
                SystemState::WifiConnecting => {
                    self.write_color(COLOR_WIFI_CONNECTING);
                    Timer::after(Duration::from_millis(200)).await;
                    self.write_color(COLOR_OFF);
                    Timer::after(Duration::from_millis(200)).await;
                }
                SystemState::EcosystemConnecting => {
                    self.write_color(COLOR_ECOSYSTEM_CONNECTING);
                    Timer::after(Duration::from_millis(250)).await;
                    self.write_color(COLOR_OFF);
                    Timer::after(Duration::from_millis(250)).await;
                }
                SystemState::Operational => {
                    self.write_color(COLOR_READY);
                    Timer::after(Duration::from_millis(80)).await;
                    self.write_color(COLOR_OFF);
                    Timer::after(Duration::from_millis(3000)).await;
                }
                SystemState::EcosystemReset => {
                    self.write_color(COLOR_HOLD_STAGE1);
                    Timer::after(Duration::from_millis(200)).await;
                    self.write_color(COLOR_OFF);
                    Timer::after(Duration::from_millis(200)).await;
                }
                SystemState::FactoryReset => {
                    self.write_color(COLOR_HOLD_STAGE2);
                    Timer::after(Duration::from_millis(200)).await;
                    self.write_color(COLOR_OFF);
                    Timer::after(Duration::from_millis(200)).await;
                }
                SystemState::Updating => {
                    self.write_color(COLOR_UPDATING);
                    Timer::after(Duration::from_millis(100)).await;
                    self.write_color(COLOR_OFF);
                    Timer::after(Duration::from_millis(100)).await;
                }
                SystemState::Error => {
                    for _ in 0..3 {
                        self.write_color(COLOR_ERROR);
                        Timer::after(Duration::from_millis(150)).await;
                        self.write_color(COLOR_OFF);
                        Timer::after(Duration::from_millis(150)).await;
                    }
                    Timer::after(Duration::from_millis(1000)).await;
                }
            }
        }
    }
}

impl HoldFeedback for DevKitLedDriver {
    fn hold_feedback_press(&self) {
        self.signal.signal(LedCommand::HoldPress);
    }

    fn hold_feedback_stage1(&self) {
        self.signal.signal(LedCommand::HoldStage1);
    }

    fn hold_feedback_stage2(&self) {
        self.signal.signal(LedCommand::HoldStage2);
    }

    fn restore_state(&self) {
        self.signal.signal(LedCommand::Restore);
    }
}

impl IntercomReporter for DevKitLedDriver {
    fn report_ring(&self, ringing: bool) {
        self.signal.signal(LedCommand::Ring(ringing));
    }

    fn report_lock_state(&self, state: LockState) {
        if matches!(state, LockState::Unlocked) {
            self.signal.signal(LedCommand::Unlock);
        }
    }

    fn report_unlock_duration(&self, _duration_sec: u32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_devkit_led_driver_hold_feedback_updates_color() {
        let driver = DevKitLedDriver::new();
        assert_eq!(driver.last_color(), COLOR_OFF);

        driver.hold_feedback_press();
        // Signal is pending, manually check write_color
        driver.write_color(COLOR_HOLD_PRESS);
        assert_eq!(driver.last_color(), COLOR_HOLD_PRESS);

        driver.write_color(COLOR_HOLD_STAGE1);
        assert_eq!(driver.last_color(), COLOR_HOLD_STAGE1);

        driver.write_color(COLOR_HOLD_STAGE2);
        assert_eq!(driver.last_color(), COLOR_HOLD_STAGE2);
    }
}
