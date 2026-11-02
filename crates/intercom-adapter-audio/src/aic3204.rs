//! TLV320AIC3204 I2C control-plane driver (TI SLAA557).
//!
//! Clocked from ESP32-C6 I2S0 at `Fs = 16 kHz` (`MCLK = 256 * Fs = 4.096 MHz`,
//! `NDAC = 1, MDAC = 2, DOSR = 128`, `NADC = 1, MADC = 2, AOSR = 128`).

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;
use embedded_hal::i2c::I2c;
use intercom_core::domain::intercom::AudioDirection;
use intercom_core::domain::policy::{RxPgaGain, TxDacGain};
use intercom_core::ports::AudioControlDriver;
use log::warn;

use crate::registers::*;

/// TLV320AIC3204 control plane with cached register-page selection.
pub struct Aic3204AudioDriver<I2C, RST> {
    i2c: I2C,
    rst: RST,
    current_page: u8,
}

/// Failure to reset or configure the codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodecError {
    I2c,
    ResetPin,
}

impl<I2C: I2c, RST: OutputPin> Aic3204AudioDriver<I2C, RST> {
    /// Takes ownership of the codec bus and active-low reset pin.
    pub const fn new(i2c: I2C, rst: RST) -> Self {
        Self {
            i2c,
            rst,
            current_page: 0xFF,
        }
    }

    /// Releases the bus and reset pin.
    pub fn into_parts(self) -> (I2C, RST) {
        (self.i2c, self.rst)
    }

    /// Resets and configures both paths, initially muted. I2S clocks must already run.
    pub async fn init(&mut self, rx: RxPgaGain, tx: TxDacGain) -> Result<(), CodecError> {
        self.rst.set_low().map_err(|_| CodecError::ResetPin)?;
        embassy_time::Timer::after(embassy_time::Duration::from_millis(2)).await;
        self.rst.set_high().map_err(|_| CodecError::ResetPin)?;
        embassy_time::Timer::after(embassy_time::Duration::from_millis(2)).await;
        self.software_reset()?;
        embassy_time::Timer::after(embassy_time::Duration::from_millis(2)).await;
        self.write_init_registers(rx, tx)
    }

    /// Blocking equivalent of [`Self::init`] with an injected delay provider.
    pub fn init_sync(
        &mut self,
        rx: RxPgaGain,
        tx: TxDacGain,
        delay: &mut impl DelayNs,
    ) -> Result<(), CodecError> {
        self.rst.set_low().map_err(|_| CodecError::ResetPin)?;
        delay.delay_ms(2);
        self.rst.set_high().map_err(|_| CodecError::ResetPin)?;
        delay.delay_ms(2);
        self.software_reset()?;
        delay.delay_ms(2);
        self.write_init_registers(rx, tx)
    }

    fn software_reset(&mut self) -> Result<(), CodecError> {
        self.select_page(0)?;
        self.write_reg(P0_SW_RESET, 0x01)?;
        // TI SLAA557 §2.3: software reset resets page register to 0; invalidate cache
        // and wait >= 1 ms (2 ms enforced in init / init_sync) before Page 1 writes.
        self.current_page = 0xFF;
        Ok(())
    }

    fn write_init_registers(&mut self, rx: RxPgaGain, tx: TxDacGain) -> Result<(), CodecError> {
        // Page 1: External 1.8V AVDD/DVDD (LDO_SELECT = GND -> AVDD LDO off, analog blocks enabled)
        self.select_page(1)?;
        self.write_reg(P1_PWR_CFG, 0x08)?; // disable weak AVDD-to-DVDD connection
        self.write_reg(P1_LDO_CTRL, 0x00)?; // analog blocks enabled (bit3=0), AVDD LDO off (bit0=0)
        self.write_reg(P1_CM_CTRL, 0x00)?; // full-chip CM = 0.9V
        self.write_reg(P1_REF_PWRUP, 0x01)?; // REF quick charge 40 ms

        // Analog routing: IN1_L (20 kOhm) -> MICPGA_L; Left DAC -> LOL (0 dB)
        self.write_reg(P1_MICPGA_L_POS, 0x80)?; // IN1_L -> MICPGA_L+ (20k)
        self.write_reg(P1_MICPGA_L_NEG, 0x80)?; // CM1L -> MICPGA_L- (20k)
        self.write_reg(P1_MICPGA_L_VOL, rx.half_db())?;
        self.write_reg(P1_LOL_ROUTING, 0x08)?; // Left DAC -> LOL
        self.write_reg(P1_LOL_GAIN, 0x00)?; // LOL unmuted, 0 dB analog gain
        self.write_reg(P1_OUT_DRV_PWR, 0x08)?; // Power up LOL only (HPL/HPR/LOR off)

        // Page 0: Direct integer clocking from MCLK = 256 * 16 kHz = 4.096 MHz -> Fs = 16.000 kHz
        self.select_page(0)?;
        self.write_reg(P0_CLK_MUX, 0x00)?; // CODEC_CLKIN = MCLK
        self.write_reg(P0_AUDIO_IF_1, 0x00)?; // Philips I2S, 16-bit word, BCLK/WCLK inputs
        self.write_reg(P0_DAC_PRB, 0x01)?; // PRB_P1
        self.write_reg(P0_ADC_PRB, 0x04)?; // PRB_R4
        self.write_reg(P0_NDAC, 0x81)?; // Power up NDAC = 1
        self.write_reg(P0_MDAC, 0x82)?; // Power up MDAC = 2
        self.write_reg(P0_DOSR_MSB, 0x00)?;
        self.write_reg(P0_DOSR_LSB, 128)?; // DOSR = 128
        self.write_reg(P0_NADC, 0x81)?; // Power up NADC = 1
        self.write_reg(P0_MADC, 0x82)?; // Power up MADC = 2
        self.write_reg(P0_AOSR, 128)?; // AOSR = 128

        // Power up Left DAC & Left ADC with soft-stepping enabled, both initially muted
        self.write_reg(P0_LDAC_VOL, tx.half_db() as u8)?;
        self.write_reg(P0_DAC_SETUP_1, 0x94)?; // Left DAC on, Right DAC off, soft-step 1/WCLK
        self.write_reg(P0_DAC_MUTE_CTRL, 0x0C)?; // Mute Left & Right DAC
        self.write_reg(P0_ADC_SETUP, 0x80)?; // Left ADC on, Right ADC off, soft-step 1/WCLK
        self.write_reg(P0_ADC_FGA_MUTE, 0x88)?; // Mute Left & Right ADC
        Ok(())
    }

    fn select_page(&mut self, page: u8) -> Result<(), CodecError> {
        if self.current_page != page {
            self.write_reg(REG_PAGE_SELECT, page)?;
            self.current_page = page;
        }
        Ok(())
    }

    fn write_reg(&mut self, reg: u8, val: u8) -> Result<(), CodecError> {
        self.i2c.write(AIC3204_I2C_ADDR, &[reg, val]).map_err(|_| {
            warn!("AIC3204 I2C write failed: reg=0x{:02x}", reg);
            CodecError::I2c
        })
    }

    fn apply_direction(&mut self, direction: AudioDirection) -> Result<(), CodecError> {
        self.select_page(0)?;
        // A failed mute must never be followed by unmuting the opposite path.
        match direction {
            AudioDirection::Muted => {
                let dac_result = self.write_reg(P0_DAC_MUTE_CTRL, 0x0C);
                let adc_result = self.write_reg(P0_ADC_FGA_MUTE, 0x88);
                dac_result.and(adc_result)
            }
            AudioDirection::CaptureRx => {
                self.write_reg(P0_DAC_MUTE_CTRL, 0x0C)?;
                self.write_reg(P0_ADC_FGA_MUTE, 0x08)
            }
            AudioDirection::PlaybackTx => {
                self.write_reg(P0_ADC_FGA_MUTE, 0x88)?;
                self.write_reg(P0_DAC_MUTE_CTRL, 0x04)
            }
        }
    }
}

impl<I2C: I2c, RST: OutputPin> AudioControlDriver for Aic3204AudioDriver<I2C, RST> {
    fn set_audio_direction(&mut self, direction: AudioDirection) {
        let _ = self.apply_direction(direction);
    }

    fn set_audio_gain(&mut self, rx: RxPgaGain, tx: TxDacGain) {
        if self.select_page(1).is_ok() {
            let _ = self.write_reg(P1_MICPGA_L_VOL, rx.half_db());
        }
        if self.select_page(0).is_ok() {
            let _ = self.write_reg(P0_LDAC_VOL, tx.half_db() as u8);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct I2cBusMock {
        writes: Vec<(u8, [u8; 2])>,
        fail_next: bool,
    }

    impl embedded_hal::i2c::ErrorType for I2cBusMock {
        type Error = embedded_hal::i2c::ErrorKind;
    }

    impl embedded_hal::i2c::I2c for I2cBusMock {
        fn transaction(
            &mut self,
            address: u8,
            operations: &mut [embedded_hal::i2c::Operation<'_>],
        ) -> Result<(), Self::Error> {
            if self.fail_next {
                self.fail_next = false;
                return Err(embedded_hal::i2c::ErrorKind::Other);
            }
            for op in operations {
                if let embedded_hal::i2c::Operation::Write(bytes) = op {
                    if bytes.len() == 2 {
                        self.writes.push((address, [bytes[0], bytes[1]]));
                    }
                }
            }
            Ok(())
        }
    }

    struct ResetPinMock {
        transitions: Vec<bool>,
    }

    impl embedded_hal::digital::ErrorType for ResetPinMock {
        type Error = core::convert::Infallible;
    }

    impl embedded_hal::digital::OutputPin for ResetPinMock {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            self.transitions.push(false);
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            self.transitions.push(true);
            Ok(())
        }
    }

    struct DelayMock {
        delays_ns: Vec<u32>,
    }

    impl DelayNs for DelayMock {
        fn delay_ns(&mut self, ns: u32) {
            self.delays_ns.push(ns);
        }
    }

    #[test]
    fn init_waits_after_software_reset_configures_external_1v8_ldo_and_starts_muted() {
        // Arrange
        let i2c = I2cBusMock {
            writes: Vec::new(),
            fail_next: false,
        };
        let rst = ResetPinMock {
            transitions: Vec::new(),
        };
        let mut delay = DelayMock {
            delays_ns: Vec::new(),
        };
        let mut driver = Aic3204AudioDriver::new(i2c, rst);

        // Act
        driver
            .init_sync(RxPgaGain::default(), TxDacGain::default(), &mut delay)
            .expect("codec init succeeds");
        let (i2c, rst) = driver.into_parts();

        // Assert
        assert_eq!(rst.transitions, vec![false, true]);
        assert_eq!(delay.delays_ns, vec![2_000_000, 2_000_000, 2_000_000]);
        assert!(i2c
            .writes
            .contains(&(AIC3204_I2C_ADDR, [P0_SW_RESET, 0x01])));
        assert!(i2c
            .writes
            .contains(&(AIC3204_I2C_ADDR, [P1_LDO_CTRL, 0x00])));
        assert!(i2c.writes.contains(&(AIC3204_I2C_ADDR, [P0_NDAC, 0x81])));
        assert!(i2c.writes.contains(&(AIC3204_I2C_ADDR, [P0_MDAC, 0x82])));
        assert!(i2c.writes.contains(&(AIC3204_I2C_ADDR, [P0_DOSR_LSB, 128])));
        assert!(i2c
            .writes
            .contains(&(AIC3204_I2C_ADDR, [P0_DAC_MUTE_CTRL, 0x0C])));
        assert!(i2c
            .writes
            .contains(&(AIC3204_I2C_ADDR, [P0_ADC_FGA_MUTE, 0x88])));
    }

    #[test]
    fn set_audio_direction_unmutes_only_active_half_duplex_path() {
        // Arrange
        let i2c = I2cBusMock {
            writes: Vec::new(),
            fail_next: false,
        };
        let rst = ResetPinMock {
            transitions: Vec::new(),
        };
        let mut delay = DelayMock {
            delays_ns: Vec::new(),
        };
        let mut driver = Aic3204AudioDriver::new(i2c, rst);
        driver
            .init_sync(RxPgaGain::default(), TxDacGain::default(), &mut delay)
            .unwrap();

        // Act
        driver.set_audio_direction(AudioDirection::CaptureRx);
        driver.set_audio_direction(AudioDirection::PlaybackTx);
        let (i2c, _) = driver.into_parts();
        let tail = &i2c.writes[i2c.writes.len() - 4..];

        // Assert
        assert_eq!(
            tail,
            &[
                (AIC3204_I2C_ADDR, [P0_DAC_MUTE_CTRL, 0x0C]),
                (AIC3204_I2C_ADDR, [P0_ADC_FGA_MUTE, 0x08]),
                (AIC3204_I2C_ADDR, [P0_ADC_FGA_MUTE, 0x88]),
                (AIC3204_I2C_ADDR, [P0_DAC_MUTE_CTRL, 0x04]),
            ]
        );
    }

    #[test]
    fn init_programs_ordered_clock_routing_gain_and_mute_registers_on_correct_pages() {
        // Arrange
        let i2c = I2cBusMock {
            writes: Vec::new(),
            fail_next: false,
        };
        let rst = ResetPinMock {
            transitions: Vec::new(),
        };
        let mut delay = DelayMock {
            delays_ns: Vec::new(),
        };
        let mut driver = Aic3204AudioDriver::new(i2c, rst);

        // Act
        driver
            .init_sync(
                RxPgaGain::try_new(95).unwrap(),
                TxDacGain::try_new(-127).unwrap(),
                &mut delay,
            )
            .unwrap();
        let (i2c, _) = driver.into_parts();

        // Assert
        let expected = [
            [REG_PAGE_SELECT, 0],
            [P0_SW_RESET, 1],
            [REG_PAGE_SELECT, 1],
            [P1_PWR_CFG, 0x08],
            [P1_LDO_CTRL, 0],
            [P1_CM_CTRL, 0],
            [P1_REF_PWRUP, 1],
            [P1_MICPGA_L_POS, 0x80],
            [P1_MICPGA_L_NEG, 0x80],
            [P1_MICPGA_L_VOL, 95],
            [P1_LOL_ROUTING, 0x08],
            [P1_LOL_GAIN, 0],
            [P1_OUT_DRV_PWR, 0x08],
            [REG_PAGE_SELECT, 0],
            [P0_CLK_MUX, 0],
            [P0_AUDIO_IF_1, 0],
            [P0_DAC_PRB, 1],
            [P0_ADC_PRB, 4],
            [P0_NDAC, 0x81],
            [P0_MDAC, 0x82],
            [P0_DOSR_MSB, 0],
            [P0_DOSR_LSB, 128],
            [P0_NADC, 0x81],
            [P0_MADC, 0x82],
            [P0_AOSR, 128],
            [P0_LDAC_VOL, 0x81],
            [P0_DAC_SETUP_1, 0x94],
            [P0_DAC_MUTE_CTRL, 0x0c],
            [P0_ADC_SETUP, 0x80],
            [P0_ADC_FGA_MUTE, 0x88],
        ]
        .map(|write| (AIC3204_I2C_ADDR, write));
        assert_eq!(i2c.writes, expected);
    }

    #[test]
    fn muted_direction_mutes_both_paths() {
        // Arrange
        let mut driver = Aic3204AudioDriver::new(
            I2cBusMock {
                writes: Vec::new(),
                fail_next: false,
            },
            ResetPinMock {
                transitions: Vec::new(),
            },
        );

        // Act
        driver.set_audio_direction(AudioDirection::Muted);
        let (i2c, _) = driver.into_parts();

        // Assert
        assert_eq!(
            i2c.writes,
            vec![
                (AIC3204_I2C_ADDR, [REG_PAGE_SELECT, 0]),
                (AIC3204_I2C_ADDR, [P0_DAC_MUTE_CTRL, 0x0c]),
                (AIC3204_I2C_ADDR, [P0_ADC_FGA_MUTE, 0x88]),
            ]
        );
    }

    #[test]
    fn gain_updates_select_analog_and_digital_pages_and_encode_signed_dac_gain() {
        // Arrange
        let mut driver = Aic3204AudioDriver::new(
            I2cBusMock {
                writes: Vec::new(),
                fail_next: false,
            },
            ResetPinMock {
                transitions: Vec::new(),
            },
        );

        // Act
        driver.set_audio_gain(
            RxPgaGain::try_new(80).unwrap(),
            TxDacGain::try_new(-20).unwrap(),
        );
        let (i2c, _) = driver.into_parts();

        // Assert
        assert_eq!(
            i2c.writes,
            vec![
                (AIC3204_I2C_ADDR, [REG_PAGE_SELECT, 1]),
                (AIC3204_I2C_ADDR, [P1_MICPGA_L_VOL, 80]),
                (AIC3204_I2C_ADDR, [REG_PAGE_SELECT, 0]),
                (AIC3204_I2C_ADDR, [P0_LDAC_VOL, 236]),
            ]
        );
    }

    #[test]
    fn failed_mute_never_unmutes_opposite_half_duplex_path() {
        // Arrange
        let mut driver = Aic3204AudioDriver::new(
            I2cBusMock {
                writes: Vec::new(),
                fail_next: false,
            },
            ResetPinMock {
                transitions: Vec::new(),
            },
        );
        driver.set_audio_direction(AudioDirection::Muted);
        driver.i2c.writes.clear();
        driver.i2c.fail_next = true;

        // Act
        driver.set_audio_direction(AudioDirection::CaptureRx);
        driver.set_audio_direction(AudioDirection::PlaybackTx);
        let (i2c, _) = driver.into_parts();

        // Assert
        assert_eq!(
            i2c.writes,
            vec![
                (AIC3204_I2C_ADDR, [P0_ADC_FGA_MUTE, 0x88]),
                (AIC3204_I2C_ADDR, [P0_DAC_MUTE_CTRL, 0x04]),
            ]
        );
    }

    #[test]
    fn failed_page_selection_is_retried_before_register_access() {
        // Arrange
        let mut driver = Aic3204AudioDriver::new(
            I2cBusMock {
                writes: Vec::new(),
                fail_next: true,
            },
            ResetPinMock {
                transitions: Vec::new(),
            },
        );

        // Act
        driver.set_audio_direction(AudioDirection::CaptureRx);
        driver.set_audio_direction(AudioDirection::CaptureRx);
        let (i2c, _) = driver.into_parts();

        // Assert
        assert_eq!(
            i2c.writes,
            vec![
                (AIC3204_I2C_ADDR, [REG_PAGE_SELECT, 0]),
                (AIC3204_I2C_ADDR, [P0_DAC_MUTE_CTRL, 0x0c]),
                (AIC3204_I2C_ADDR, [P0_ADC_FGA_MUTE, 0x08]),
            ]
        );
    }
}
