use embassy_time::{Duration, Instant, Timer};
use embedded_hal_async::spi::{ErrorKind, SpiDevice};
use firmware_common::driver::barometer::{BaroReading, Barometer};
use embedded_hal_async::spi::Error;
use crate::sleep;


#[derive(Clone, Copy)]
struct Coefficients {
    sens_t1: u16,
    off_t1: u16,
    tcs: u16,
    tco: u16,
    t_ref: u16,
    tempsens: u16,
}

pub struct MS5607<B: SpiDevice> {
    spi: B,
    coefficients: Option<Coefficients>,
}

impl<B: SpiDevice> MS5607<B> {
    pub fn new(spi_device: B) -> Self {
        Self {
            spi:spi_device,
            coefficients: None,
        }
    }
}

impl<B: SpiDevice> Barometer for MS5607<B> {
    type Error = ErrorKind;

    async fn reset(&mut self) -> Result<(), ErrorKind> {
        // reset
        self.spi.transfer(&mut [0u8], &[0x1E]).await.map_err(|e|e.kind())?;

        sleep!(20);

        // read coefficients
        let mut coefficients = [0u16; 6];
        for addr in 1..=6 {
            let write_data: [u8; 3] = [0xA0 | (addr << 1), 0, 0];
            let mut read_buffer = [0; 3];
            self.spi
                .transfer(&mut read_buffer, &write_data)
                .await.map_err(|e|e.kind())?;

            coefficients[(addr - 1) as usize] =
                ((read_buffer[1] as u16) << 8) | (read_buffer[2] as u16);
        }
        self.coefficients = Some(Coefficients {
            sens_t1: coefficients[0],
            off_t1: coefficients[1],
            tcs: coefficients[2],
            tco: coefficients[3],
            t_ref: coefficients[4],
            tempsens: coefficients[5],
        });

        Ok(())
    }

    async fn read(&mut self) -> Result<BaroReading, ErrorKind> {
        // request measurement pressure with OSR=1024
        let timestamp = Instant::now().as_micros() as f64 / 1000.0 + 1.0; // timestamp of the pressure measurement
        self.spi.transfer(&mut [0u8], &[0x44]).await.map_err(|e|e.kind())?;
        Timer::after(Duration::from_micros(2280)).await;

        // read pressure measurement
        let write_data: [u8; 4] = [0x00, 0, 0, 0];
        let mut read_buffer = [0; 4];
        self.spi
            .transfer(&mut read_buffer, &write_data)
            .await.map_err(|e|e.kind())?;
        let d1 = ((read_buffer[1] as u32) << 16)
            | ((read_buffer[2] as u32) << 8)
            | (read_buffer[3] as u32);

        // request measurement temperature with OSR=256
        self.spi.transfer(&mut [0u8], &[0x50]).await.map_err(|e|e.kind())?;
        Timer::after(Duration::from_micros(600)).await;

        // read temerature measurement
        self.spi
            .transfer(&mut read_buffer, &write_data)
            .await.map_err(|e|e.kind())?;
        let d2 = ((read_buffer[1] as u32) << 16)
            | ((read_buffer[2] as u32) << 8)
            | (read_buffer[3] as u32);

        let coeffs = self.coefficients.unwrap();

        // temperature calculation
        let dt: i64 = ((d2 as i32) - ((coeffs.t_ref as i32) << 8)) as i64;
        let mut temperature = (2000i64 + ((dt as i64 * coeffs.tempsens as i64) >> 23)) as i32;

        // compensated pressure calculation
        let mut off = ((coeffs.off_t1 as i64) << 17) + (((coeffs.tco as i64) * dt) >> 6);
        let mut sens = ((coeffs.sens_t1 as i64) << 16) + (((coeffs.tcs as i64) * dt) >> 7);

        // second order temperature compensation
        if temperature < 2000 {
            let t2 = ((dt * dt) >> 31) as i32;
            let mut off2 = (61 * (temperature - 2000) * (temperature - 2000)) >> 4;
            let mut sens2 = 2 * (temperature - 2000) * (temperature - 2000);

            if temperature < -1500 {
                off2 += 15 * (temperature + 1500) * (temperature + 1500);
                sens2 += 8 * (temperature + 1500) * (temperature + 1500);
            }

            temperature -= t2;
            off -= off2 as i64;
            sens -= sens2 as i64;
        }

        // pressure calculation
        let pressure = (((((d1 as i64) * sens) >> 21) - off) >> 15) as i32;

        Ok(BaroReading {
            timestamp,
            temperature: temperature as f32 / 100.0,
            pressure: pressure as f32,
        })
    }
}