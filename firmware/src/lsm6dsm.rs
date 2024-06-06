use embassy_time::Instant;
use embedded_hal_async::spi::Error;
use embedded_hal_async::spi::{ErrorKind, SpiDevice};
use firmware_common::driver::imu::{IMUReading, IMU};
use firmware_common::driver::timestamp::BootTimestamp;

use crate::sleep;

const WHO_AM_I: u8 = 0x0F;
const STATUS_REG: u8 = 0x1E;
const CTRL1_XL: u8 = 0x10;
const CTRL2_G: u8 = 0x11;
const CTRL3_C: u8 = 0x12;
const OUTX_L_G: u8 = 0x22;

pub struct LSM6DSM<B: SpiDevice> {
    spi: B,
}

impl<B: SpiDevice> LSM6DSM<B> {
    pub fn new(spi_device: B) -> Self {
        Self { spi: spi_device }
    }

    pub async fn verify_identity(&mut self) -> Result<u8, ErrorKind> {
        let id = self.read_register(WHO_AM_I).await?;
        Ok(id)
    }

    pub async fn check_status(&mut self) -> Result<u8, ErrorKind> {
        let status = self.read_register(STATUS_REG).await?;
        Ok(status)
    }

    /*
       Set the sensor to low power mode

       CTRL1_XL (10h) - 0b0011_11_00;  Set odr to 52Hz, full scale to 16g, low cutoff freq, high BW
       CTRL2_G (11h) - 0b0011_11_00;   Set odr to 52Hz, full scale to 2000dps, low cutoff freq, high BW
    */
    pub async fn low_power(&mut self) -> Result<(), ErrorKind> {
        self.write_register(CTRL1_XL, 0b0011_1100).await?;
        sleep!(10);
        self.write_register(CTRL2_G, 0b0011_1100).await?;
        sleep!(10);
        Ok(())
    }

    /*
       Set the sensor to normal mode (out of low power mode)

       CTRL1_XL (10h) - 0b1010_11_00; Set odr (acceleration) to 6.66KHz, full scale to 16g, low cutoff freq, high BW
       CTRL2_G (11h) - 0b1010_11_00;  Set odr (Gyro) to 6.66KHz, full scale to 2000dps, low cutoff freq, high BW
    */
    pub async fn normal_mode(&mut self) -> Result<(), ErrorKind> {
        self.write_register(CTRL1_XL, 0b1010_1100).await?;
        sleep!(10);
        self.write_register(CTRL2_G, 0b1010_1100).await?;
        sleep!(10);
        Ok(())
    }

    async fn read_register(&mut self, address: u8) -> Result<u8, ErrorKind> {
        let mut buffer = [0u8; 2];

        self.spi
            .transfer(&mut buffer, &[address | 0b10000000, 0x00])
            .await
            .map_err(|e| e.kind())?;

        Ok(buffer[1])
    }

    async fn write_register(&mut self, address: u8, value: u8) -> Result<(), ErrorKind> {
        self.spi
            .transfer(&mut [0u8; 2], &[address & !0b10000000, value])
            .await
            .map_err(|e| e.kind())?;
        Ok(())
    }
}

impl<B: SpiDevice> IMU for LSM6DSM<B> {
    type Error = ErrorKind;

    /*
       Reset the sensor

       CTRL1_XL (10h) - 0b1010_11_00; Set odr (acceleration) to 6.66KHz, full scale to 16g, low cutoff freq, high BW
       CTRL2_G (11h) - 0b1010_11_00;  Set odr (Gyro) to 6.66KHz, full scale to 2000dps, low cutoff freq, high BW
       CTRL3_C (12h) - 0b1100_01_01;  Reboots memory content/software, 4-wire SPI, enable block data update
    */
    async fn reset(&mut self) -> Result<(), Self::Error> {
        self.write_register(CTRL3_C, 0b10000101).await?;
        sleep!(10);

        let id = self.verify_identity().await?;
        if id != 0x6A {
            return Err(ErrorKind::Other);
        }

        self.write_register(CTRL1_XL, 0b10101100).await?;
        sleep!(10);
        self.write_register(CTRL2_G, 0b10101100).await?;
        sleep!(10);

        Ok(())
    }

    async fn read(&mut self) -> Result<IMUReading<BootTimestamp>, Self::Error> {
        let status = self.check_status().await?;
        let new_gyro_data = status & 0x02 != 0;
        let new_accel_data = status & 0x01 != 0;

        if !new_gyro_data && !new_accel_data {
            return Err(ErrorKind::Other);
        }

        let mut buffer = [0u8; 13];
        self.spi
            .transfer(
                &mut buffer,
                &[OUTX_L_G | 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            )
            .await
            .map_err(|e| e.kind())?;

        let buffer = &buffer[1..];
        let gyro_x = i16::from_le_bytes([buffer[0], buffer[1]]);
        let gyro_y = i16::from_le_bytes([buffer[2], buffer[3]]);
        let gyro_z = i16::from_le_bytes([buffer[4], buffer[5]]);

        let acc_x = i16::from_le_bytes([buffer[6], buffer[7]]);
        let acc_y = i16::from_le_bytes([buffer[8], buffer[9]]);
        let acc_z = i16::from_le_bytes([buffer[10], buffer[11]]);

        let acc_scale = 16.0 / 32768.0; // ±16g range
        let gyro_scale = 2000.0 / 32768.0; // ±2000dps range

        Ok(IMUReading::new(
            Instant::now().as_micros() as f64 / 1000.0,
            [
                acc_x as f32 * acc_scale,
                acc_y as f32 * acc_scale,
                acc_z as f32 * acc_scale,
            ],
            [
                gyro_x as f32 * gyro_scale,
                gyro_y as f32 * gyro_scale,
                gyro_z as f32 * gyro_scale,
            ],
        ))
    }
}
