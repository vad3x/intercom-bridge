//! ESP32-S31 board GPIO pin assignments.

/// S31 board GPIO assignments. Keep these together when aligning firmware to a PCB revision.
pub const RING_INPUT: u8 = 2;
pub const BUS_LISTEN: u8 = 7;
pub const BOOT_BUTTON: u8 = 9;
pub const BUS_TALK: u8 = 10;
pub const CODEC_RESET_N: u8 = 11;
pub const I2S_MCLK: u8 = 15;
pub const BUS_DOOR: u8 = 18;
pub const INDICATOR_LED: u8 = 19;
pub const I2S_BCLK: u8 = 20;
pub const I2S_WS: u8 = 21;
pub const I2S_DOUT: u8 = 22;
pub const I2S_DIN: u8 = 23;
pub const CODEC_I2C_SDA: u8 = 0;
pub const CODEC_I2C_SCL: u8 = 1;
