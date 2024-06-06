use defmt::info;
use embassy_time::Instant;
use embedded_hal_async::i2c::Error;
use embedded_hal_async::i2c::{ErrorKind, I2c};
use firmware_common::driver::meg::{MegReading, Megnetometer};
use firmware_common::driver::timestamp::BootTimestamp;

use crate::sleep;

const ADDRESS: u8 = 0x30;

const CTRL_REG_0: u8 = 0x1b;
const CTRL_REG_1: u8 = 0x1c;
const CTRL_REG_2: u8 = 0x1d;
const ODR_REG: u8 = 0x1a;

pub struct MMC5603<B: I2c> {
    i2c: B,
}

impl<B: I2c> MMC5603<B> {
    pub fn new(i2c: B) -> Self {
        Self { i2c }
    }

    async fn write_reg(&mut self, reg: u8, data: u8) -> Result<(), ErrorKind> {
        self.i2c
            .write(ADDRESS, &[reg, data])
            .await
            .map_err(|e| e.kind())
    }
}

impl<B: I2c> Megnetometer for MMC5603<B> {
    type Error = ErrorKind;

    // reset and config the meg to 75Hz, auto set/reset
    async fn reset(&mut self) -> Result<(), ErrorKind> {
        // software reset
        info!("resetting meg");

        self.write_reg(CTRL_REG_1, 0b10000000).await?;
        // wait for device
        sleep!(20);

        info!("set ODR register to 75Hz");
        // set ODR register to 75Hz
        self.write_reg(ODR_REG, 75).await?;

        // set Cmm_freq_en and Auto_SR_en to 1
        self.write_reg(CTRL_REG_0, 0b10100000).await?;
        // wait for calculation
        sleep!(5);

        // set Cmm_en to 1
        self.write_reg(CTRL_REG_2, 0b00010000).await?;
        // wait for the first measurement
        sleep!(7);

        Ok(())
    }

    // unit: Gauss
    async fn read(&mut self) -> Result<MegReading<BootTimestamp>, ErrorKind> {
        let timestamp = Instant::now().as_micros() as f64 / 1000.0;

        let mut read_buffer = [0; 9];
        self.i2c
            .write_read(ADDRESS, &[0], &mut read_buffer)
            .await
            .map_err(|e| e.kind())?;

        let x: u32 = u32::from(read_buffer[0]) << 12
            | u32::from(read_buffer[1]) << 4
            | u32::from(read_buffer[6]) >> 4;
        let x = (x as f32 * 0.0625f32 - 32768f32) / 1000.0;
        let y: u32 = u32::from(read_buffer[2]) << 12
            | u32::from(read_buffer[3]) << 4
            | u32::from(read_buffer[7]) >> 4;
        let y = (y as f32 * 0.0625f32 - 32768f32) / 1000.0;
        let z: u32 = u32::from(read_buffer[4]) << 12
            | u32::from(read_buffer[5]) << 4
            | u32::from(read_buffer[8]) >> 4;
        let z = (z as f32 * 0.0625f32 - 32768f32) / 1000.0;

        Ok(MegReading::new(timestamp, [x, y, z]))
    }
}
