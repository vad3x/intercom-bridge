//! TLV320AIC3204 I2C address and Page 0 / Page 1 register constants (TI SLAA557).

pub const AIC3204_I2C_ADDR: u8 = 0x18;

pub const REG_PAGE_SELECT: u8 = 0x00;

// Page 0 Registers
pub const P0_SW_RESET: u8 = 0x01;
pub const P0_CLK_MUX: u8 = 0x04;
pub const P0_NDAC: u8 = 0x0B;
pub const P0_MDAC: u8 = 0x0C;
pub const P0_DOSR_MSB: u8 = 0x0D;
pub const P0_DOSR_LSB: u8 = 0x0E;
pub const P0_NADC: u8 = 0x12;
pub const P0_MADC: u8 = 0x13;
pub const P0_AOSR: u8 = 0x14;
pub const P0_AUDIO_IF_1: u8 = 0x1B;
pub const P0_DAC_PRB: u8 = 0x3C;
pub const P0_ADC_PRB: u8 = 0x3D;
pub const P0_DAC_SETUP_1: u8 = 0x3F;
pub const P0_DAC_MUTE_CTRL: u8 = 0x40;
pub const P0_LDAC_VOL: u8 = 0x41;
pub const P0_ADC_SETUP: u8 = 0x51;
pub const P0_ADC_FGA_MUTE: u8 = 0x52;

// Page 1 Registers
pub const P1_PWR_CFG: u8 = 0x01;
pub const P1_LDO_CTRL: u8 = 0x02;
pub const P1_CM_CTRL: u8 = 0x0A;
pub const P1_LOL_ROUTING: u8 = 0x0E;
pub const P1_OUT_DRV_PWR: u8 = 0x09;
pub const P1_LOL_GAIN: u8 = 0x12;
pub const P1_MICPGA_L_POS: u8 = 0x34;
pub const P1_MICPGA_L_NEG: u8 = 0x36;
pub const P1_MICPGA_L_VOL: u8 = 0x3B;
pub const P1_REF_PWRUP: u8 = 0x7B;
