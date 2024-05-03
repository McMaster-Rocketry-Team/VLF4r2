// Driver for lsmd6dsm accelerometer and gyroscope sensor

use embassy_time::Instant;
use embedded_hal_async::spi::Error;
use embedded_hal_async::spi::{ErrorKind, SpiDevice};
use firmware_common::driver::imu::{IMUReading, IMU};

use crate::sleep;

const CTRL1_XL : u8 = 0x10;
const CTRL2_G : u8 = 0x11;
const CTRL3_C : u8 = 0x12;
const OUTX_L_G: u8 = 0x22;
 

pub struct LSM6DSM<B: SpiDevice> {
    spi: B,
}

impl<B: SpiDevice> LSM6DSM<B> {
    pub fn new(spi_device: B) -> Self {
        Self { spi: spi_device }
    }

    pub async fn read_register(&mut self, address: u8) -> Result<u8, ErrorKind> {
        let mut buffer = [0u8; 2];

        self.spi
            .transfer(&mut buffer, &[address | 0b10000000, 0x00])
            .await
            .map_err(|e| e.kind())?;

        Ok(buffer[1])
    }

    pub async fn write_register(&mut self, address: u8, value: u8) -> Result<(), ErrorKind> {
        self.spi
            .transfer(&mut [0u8; 2], &[address & !0b10000000, value])
            .await
            .map_err(|e| e.kind())?;
        Ok(())
}

impl<B: SpiDevice> IMU for LSM6DSM<B> {
    type Error = ErrorKind;

    /*
        Reset the sensor

        CTRL1_XL (10h) - 0b1010_11_00; Set odr (acceleration) to 6.66KHz, full scale to 16g, low cutoff freq, high BW
        CTRL2_G (11h) - 0b1010_11_00;  Set odr (Gyro) to 6.66KHz, full scale to 2000dps, low cutoff freq, high BW
        CTRL3_C (12h) - 0b1100_01_01;  Reboots memory content/software, 4-wire SPI, enable block data update
     */
    pub async fn reset(&mut self) -> Result<(), Self::Error> {
        self.write_register(CTRL1_XL, 0b1010_1100).await?;
        sleep!(10);
        self.write_register(CTRL2_G, 0b1010_1100).await?;
        sleep!(10);
        self.write_register(CTRL3_C, 0b1100_0101).await?;
        sleep!(10);
        Ok(())
    }

    /*
        Set the sensor to low power mode

        CTRL1_XL (10h) - 0b0011_11_00;  Set odr to 52Hz, full scale to 16g, low cutoff freq, high BW
        CTRL2_G (11h) - 0b0011_11_00;   Set odr to 52Hz, full scale to 2000dps, low cutoff freq, high BW
     */
    pub async fn low_power(&mut self) -> Result<(), Self::Error> {
        self.write_register(CTRL1_XL, 0b0011_1100).await?;
        sleep!(10);
        self.write_register(CTRL2_G, 0b0011_1100).await?;
        sleep!(10);
        Ok(())
    }

    pub async fn read(&mut self) -> Result<IMUReading, Self::Error> {
        let mut buffer = [0u8; 12];

        self.spi
            .transfer(&mut buffer, &[OUTX_L_G | 0b10000000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
            .await
            .map_err(|e| e.kind())?;

        let gyro_x = i16::from_le_bytes([buffer[0], buffer[1]]);
        let gyro_y = i16::from_le_bytes([buffer[2], buffer[3]]);
        let gyro_z = i16::from_le_bytes([buffer[4], buffer[5]]);

        let acc_x = i16::from_le_bytes([buffer[6], buffer[7]]);
        let acc_y = i16::from_le_bytes([buffer[8], buffer[9]]);
        let acc_z = i16::from_le_bytes([buffer[10], buffer[11]]);

        let acc_scale = 16.0 / 32768.0; // ±16g range
        let gyro_scale = 2000.0 / 32768.0; // ±2000dps range

        let imu_reading = IMUReading {
            timestamp: Instant::now(),
            acc: [
                acc_x as f32 * acc_scale,
                acc_y as f32 * acc_scale,
                acc_z as f32 * acc_scale,
            ],
            gyro: [
                gyro_x as f32 * gyro_scale,
                gyro_y as f32 * gyro_scale,
                gyro_z as f32 * gyro_scale,
            ],
        };

        Ok(imu_reading)
    }
}
}