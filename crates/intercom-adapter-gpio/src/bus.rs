//! Synchronous 1-hot GPIO driver for PCB v2 PhotoMOS relays.
//!
//! U14 selects Listen, U15 selects Talk, and U7 selects DoorUnlock.
//! The domain controller supplies the relay settling interval between modes.

use esp_hal::gpio::{Level, Output, OutputConfig, OutputPin};
use intercom_core::domain::intercom::BusLineState;
use intercom_core::ports::BusSwitchDriver;

/// Drives exactly one selected relay, or clears all three in the idle state.
pub struct GpioBusDriver {
    listen: Output<'static>,
    talk: Output<'static>,
    door: Output<'static>,
}

impl GpioBusDriver {
    /// Take ownership of the relay outputs and initialize them all LOW.
    pub fn new(config: GpioBusConfig) -> Self {
        let mut driver = Self {
            listen: config.listen,
            talk: config.talk,
            door: config.door,
        };
        driver.all_low();
        driver
    }

    fn all_low(&mut self) {
        self.listen.set_low();
        self.talk.set_low();
        self.door.set_low();
    }
}

/// Active-HIGH PCB v2 PhotoMOS control outputs, initialized LOW.
pub struct GpioBusConfig {
    /// Listen relay control (IO_LISTEN, GPIO7).
    pub listen: Output<'static>,
    /// Talk relay control (IO_TALK, GPIO10).
    pub talk: Output<'static>,
    /// Door strike relay control (IO_DOOR, GPIO18).
    pub door: Output<'static>,
}

impl GpioBusConfig {
    /// Configure all three relay pins as outputs with an initial LOW level.
    pub fn new(
        listen_pin: impl OutputPin + 'static,
        talk_pin: impl OutputPin + 'static,
        door_pin: impl OutputPin + 'static,
    ) -> Self {
        Self {
            listen: Output::new(listen_pin, Level::Low, OutputConfig::default()),
            talk: Output::new(talk_pin, Level::Low, OutputConfig::default()),
            door: Output::new(door_pin, Level::Low, OutputConfig::default()),
        }
    }
}

impl BusSwitchDriver for GpioBusDriver {
    fn set_bus_state(&mut self, state: BusLineState) {
        match state {
            BusLineState::Idle => self.all_low(),
            BusLineState::Listen => {
                self.talk.set_low();
                self.door.set_low();
                self.listen.set_high();
            }
            BusLineState::Talk => {
                self.listen.set_low();
                self.door.set_low();
                self.talk.set_high();
            }
            BusLineState::DoorUnlock => {
                self.listen.set_low();
                self.talk.set_low();
                self.door.set_high();
            }
        }
    }
}
