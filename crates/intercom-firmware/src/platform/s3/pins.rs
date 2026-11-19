//! ESP32-S3-N16R8 GPIO pin assignments and relay clear masks.

/// ESP32-S3 board signal assignments for the N16R8 dev board profile.
pub struct BoardPins {
    pub ring_input: u8,
    pub bus_listen: u8,
    pub boot_button: u8,
    pub bus_talk: u8,
    pub codec_reset_n: u8,
    pub i2s_mclk: u8,
    pub bus_door: u8,
    pub indicator_led: u8,
    pub i2s_bclk: u8,
    pub i2s_ws: u8,
    pub i2s_dout: u8,
    pub i2s_din: u8,
    pub codec_i2c_sda: u8,
    pub codec_i2c_scl: u8,
}

impl BoardPins {
    /// Candidate assignments avoid N16R8 memory, console, and flash pins. The
    /// supplied DevKitC pinout routes its WS2812 RGB LED to GPIO47.
    pub const SUPPORTED: Self = Self {
        ring_input: 4,
        bus_listen: 17,
        boot_button: 16,
        bus_talk: 18,
        codec_reset_n: 21,
        i2s_mclk: 10,
        bus_door: 40,
        indicator_led: 47,
        i2s_bclk: 11,
        i2s_ws: 12,
        i2s_dout: 13,
        i2s_din: 14,
        codec_i2c_sda: 8,
        codec_i2c_scl: 9,
    };

    /// Return every GPIO used by the selected board profile.
    pub const fn all_signal_pins(&self) -> [u8; 14] {
        [
            self.ring_input,
            self.bus_listen,
            self.boot_button,
            self.bus_talk,
            self.codec_reset_n,
            self.i2s_mclk,
            self.bus_door,
            self.indicator_led,
            self.i2s_bclk,
            self.i2s_ws,
            self.i2s_dout,
            self.i2s_din,
            self.codec_i2c_sda,
            self.codec_i2c_scl,
        ]
    }
}

/// Pins that the board profile must keep available for application wiring.
#[cfg(test)]
pub const EXPOSED_PINS: &[u8] = &[4, 8, 9, 10, 11, 12, 13, 14, 16, 17, 18, 21, 40, 47, 48];

/// GPIOs reserved for module memory, flash, USB/UART consoles, or boot strapping.
#[cfg(test)]
pub const RESERVED_PINS: &[u8] = &[
    0, 3, 19, 20, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 43, 44, 45, 46,
];

/// Relay outputs that must be cleared together during reset and panic handling.
pub struct RelayPins {
    pub listen: u8,
    pub talk: u8,
    pub door: u8,
}

/// GPIO bank masks for the S3 write-one-to-clear registers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpioClearMasks {
    pub gpio_low: u32,
    pub gpio_high: u32,
}

impl From<RelayPins> for GpioClearMasks {
    fn from(pins: RelayPins) -> Self {
        let mut masks = Self {
            gpio_low: 0,
            gpio_high: 0,
        };
        for pin in [pins.listen, pins.talk, pins.door] {
            if pin < 32 {
                masks.gpio_low |= 1 << pin;
            } else {
                masks.gpio_high |= 1 << (pin - 32);
            }
        }
        masks
    }
}
