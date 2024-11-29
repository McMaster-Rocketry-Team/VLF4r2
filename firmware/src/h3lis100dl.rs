use embassy_time::Instant;
use embedded_hal_async::spi::Error;
use embedded_hal_async::spi::{ErrorKind, SpiDevice};
use firmware_common::common::sensor_reading::SensorReading;
use firmware_common::driver::imu::{IMUData, IMU};
use firmware_common::driver::timestamp::BootTimestamp;

use crate::sleep;

const CTRL_REG1: u8 = 0x20;
const OUT_X_REG: u8 = 0x29;
const OUT_Y_REG: u8 = 0x2B;
const OUT_Z_REG: u8 = 0x2D;

pub struct H3LIS100DL<B: SpiDevice> {
    spi: B,
}

impl<B: SpiDevice> H3LIS100DL<B> {
    pub fn new(spi_device: B) -> Self {
        Self { spi: spi_device }
    }

    async fn read_register(&mut self, address: u8) -> Result<u8, ErrorKind> {
        let mut buffer = [0u8; 2];

        self.spi
            .transfer(&mut buffer, &[address | 0b10000000, 0x00])
            .await
            .map_err(|e| e.kind())?;

        Ok(buffer[1])
    }
}

impl<B: SpiDevice> IMU for H3LIS100DL<B> {
    type Error = ErrorKind;

    async fn reset(&mut self) -> Result<(), Self::Error> {
        // power on, set ODR to 400HZ, enable all axes
        self.spi
            .transfer(&mut [0u8; 2], &[CTRL_REG1, 0b0011_0111])
            .await
            .map_err(|e| e.kind())?;
        sleep!(10);
        Ok(())
    }

    async fn read(&mut self) -> Result<SensorReading<BootTimestamp, IMUData>, Self::Error> {
        let timestamp = Instant::now().as_micros() as f64 / 1000.0;
        // TODO merge into one read?
        let x = self.read_register(OUT_X_REG).await? as i8;
        let y = self.read_register(OUT_Y_REG).await? as i8;
        let z = self.read_register(OUT_Z_REG).await? as i8;

        let x = (x as f32 * 100.0 / 127.0) * 9.81;
        let y = (y as f32 * 100.0 / 127.0) * 9.81;
        let z = (z as f32 * 100.0 / 127.0) * 9.81;

        Ok(SensorReading::new(
            timestamp,
            IMUData {
                acc: [x, y, z],
                gyro: [0.0, 0.0, 0.0],
            },
        ))
    }
}
