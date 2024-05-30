use core::cmp::min;

use embassy_stm32::crc::{Config as CrcConfig, Crc, InputReverseConfig, PolySize};
use embassy_stm32::gpio::Output;
use embassy_stm32::peripherals::CRC as PeriCRC;
use embassy_time::{Duration, Ticker, Timer};
use embedded_hal_async::spi::{Error as SpiError, ErrorKind, SpiDevice};
use vlfs::{AsyncEraseFlash, Crc as VLFSCrc, Flash as VLFSFlash};

use crate::checkBit;

pub struct CrcWrapper<'a> {
    crc: Crc<'a>,
    last_checksum: u32,
}

impl<'a> CrcWrapper<'a> {
    pub fn new(crc: PeriCRC) -> Self {
        let config =
            CrcConfig::new(InputReverseConfig::None, false, PolySize::Width8, 69, 69).unwrap();
        Self {
            crc: Crc::new(crc, config),
            last_checksum: 0,
        }
    }
}

impl<'a> VLFSCrc for CrcWrapper<'a> {
    fn reset(&mut self) {
        self.crc.reset();
    }

    fn feed(&mut self, word: u32) {
        self.last_checksum = self.crc.feed_word(word);
    }

    fn read(&self) -> u32 {
        self.last_checksum
    }
}

#[derive(defmt::Format, Debug)]
pub enum SpiFlashError {
    BusError(ErrorKind),
    WaitBusyTimeout { ms: u64 },
}

impl From<ErrorKind> for SpiFlashError {
    fn from(value: ErrorKind) -> Self {
        Self::BusError(value)
    }
}

#[derive(defmt::Format)]
pub struct StatusRegister1 {
    busy: bool,
    write_enable_latch: bool,
}

pub struct SpiFlash<B: SpiDevice> {
    n_reset: Output<'static>,
    spi: B,
    temp_buffer: [u8; 4101],
}

impl<B: SpiDevice> SpiFlash<B> {
    // must call enter_4_byte_mode after this
    pub fn new(spi_device: B, n_reset: Output<'static>) -> Self {
        Self {
            spi: spi_device,
            n_reset,
            temp_buffer: [0u8; 4101],
        }
    }

    pub async fn enter_4_byte_mode(&mut self) -> Result<(), ErrorKind> {
        let write_data: [u8; 1] = [0xB7];
        let mut read_buffer = [0u8; 1];
        self.spi
            .transfer(&mut read_buffer, &write_data)
            .await
            .map_err(|e| e.kind())
    }

    async fn enable_write(&mut self) -> Result<(), ErrorKind> {
        let write_data: [u8; 1] = [0x06];
        let mut read_buffer = [0u8; 1];
        self.spi
            .transfer(&mut read_buffer, &write_data)
            .await
            .map_err(|e| e.kind())
    }

    async fn read_status_register_1(&mut self) -> Result<StatusRegister1, ErrorKind> {
        let write_data: [u8; 2] = [0x05, 0x00];
        let mut read_buffer = [0u8; 2];
        self.spi
            .transfer(&mut read_buffer, &write_data)
            .await
            .map_err(|e| e.kind())?;

        Ok(StatusRegister1 {
            busy: checkBit!(read_buffer[1], 0b1),
            write_enable_latch: checkBit!(read_buffer[1], 0b10),
        })
    }

    async fn wait_till_not_busy(
        &mut self,
        tick_duration: Duration,
        timeout: Duration,
    ) -> Result<(), SpiFlashError> {
        let mut ticker = Ticker::every(tick_duration);
        let mut count = 0;
        loop {
            ticker.next().await;
            let status_register_1 = self.read_status_register_1().await?;
            if !status_register_1.busy {
                break;
            }
            count += 1;
            if tick_duration * count > timeout {
                return Err(SpiFlashError::WaitBusyTimeout {
                    ms: (tick_duration * count).as_millis(),
                });
            }
        }

        Ok(())
    }
}

impl<B> VLFSFlash for SpiFlash<B>
where
    B: SpiDevice,
{
    type Error = SpiFlashError;

    async fn size(&self) -> u32 {
        262144 * 256
    }

    async fn reset(&mut self) -> Result<(), SpiFlashError> {
        self.n_reset.set_low();
        Timer::after(Duration::from_micros(1)).await;
        self.n_reset.set_high();
        Timer::after(Duration::from_micros(60)).await;
        self.enter_4_byte_mode().await?;
        Ok(())
    }

    async fn erase_sector_4kib(&mut self, address: u32) -> Result<(), SpiFlashError> {
        self.enable_write().await?;

        let mut write_data: [u8; 5] = [0x21, 0, 0, 0, 0];
        (&mut write_data[1..5]).copy_from_slice(&address.to_be_bytes());

        let mut read_buffer = [0u8; 5];

        self.spi
            .transfer(&mut read_buffer, &write_data)
            .await
            .map_err(|e| e.kind())?;

        self.wait_till_not_busy(Duration::from_millis(5), Duration::from_millis(400))
            .await?;
        Ok(())
    }

    async fn erase_block_32kib(&mut self, address: u32) -> Result<(), SpiFlashError> {
        self.enable_write().await?;

        let mut write_data: [u8; 5] = [0x52, 0, 0, 0, 0];
        (&mut write_data[1..5]).copy_from_slice(&address.to_be_bytes());

        let mut read_buffer = [0u8; 5];

        self.spi
            .transfer(&mut read_buffer, &write_data)
            .await
            .map_err(|e| e.kind())?;

        self.wait_till_not_busy(Duration::from_millis(5), Duration::from_millis(1600))
            .await?;
        Ok(())
    }

    async fn erase_block_64kib(&mut self, address: u32) -> Result<(), SpiFlashError> {
        self.enable_write().await?;

        let mut write_data: [u8; 5] = [0xDC, 0, 0, 0, 0];
        (&mut write_data[1..5]).copy_from_slice(&address.to_be_bytes());

        let mut read_buffer = [0u8; 5];

        self.spi
            .transfer(&mut read_buffer, &write_data)
            .await
            .map_err(|e| e.kind())?;

        self.wait_till_not_busy(Duration::from_millis(5), Duration::from_millis(2000))
            .await?;

        Ok(())
    }

    // maximum read length is 4 kib
    // size of the buffer must be at least 5 bytes larger than read_length
    async fn read_4kib<'b>(
        &mut self,
        address: u32,
        read_length: usize,
        read_buffer: &'b mut [u8],
    ) -> Result<&'b [u8], SpiFlashError> {
        self.temp_buffer[0] = 0x13;
        (&mut self.temp_buffer[1..5]).copy_from_slice(&address.to_be_bytes());

        let first_5_bytes: [u8; 5] = read_buffer[0..5].try_into().unwrap();

        self.spi
            .transfer(
                &mut read_buffer[0..(read_length + 5)],
                &self.temp_buffer[0..(read_length + 5)],
            )
            .await
            .map_err(|e| e.kind())?;

        (&mut read_buffer[0..5]).copy_from_slice(&first_5_bytes);
        Ok(&read_buffer[5..(read_length + 5)])
    }

    // Write a full page of 256 bytes, the last byte of the address is ignored
    // The write buffer must be less than or equals 261 bytes long, where the last 256 bytes are the data to write
    async fn write_256b<'b>(
        &mut self,
        address: u32,
        write_buffer: &'b mut [u8],
    ) -> Result<(), SpiFlashError> {
        self.enable_write().await?;

        let first_5_bytes: [u8; 5] = write_buffer[0..5].try_into().unwrap();

        write_buffer[0] = 0x12;
        (&mut write_buffer[1..5]).copy_from_slice(&address.to_be_bytes());
        write_buffer[4] = 0;

        let transfer_len = min(261, write_buffer.len());
        self.spi
            .transfer(
                &mut self.temp_buffer[0..transfer_len],
                &write_buffer[0..transfer_len],
            )
            .await
            .map_err(|e| e.kind())?;

        (&mut write_buffer[0..5]).copy_from_slice(&first_5_bytes);

        self.wait_till_not_busy(Duration::from_micros(500), Duration::from_millis(4))
            .await?;
        Ok(())
    }
}

impl<B> AsyncEraseFlash for SpiFlash<B>
where
    B: SpiDevice,
{
    type Error = SpiFlashError;

    async fn size(&self) -> u32 {
        262144 * 256
    }

    async fn reset(&mut self) -> Result<(), SpiFlashError> {
        self.n_reset.set_low();
        Timer::after(Duration::from_micros(1)).await;
        self.n_reset.set_high();
        Timer::after(Duration::from_micros(60)).await;
        self.enter_4_byte_mode().await?;
        Ok(())
    }

    async fn erase_sector_4kib_nb(&mut self, address: u32) -> Result<(), SpiFlashError> {
        self.enable_write().await?;

        let mut write_data: [u8; 5] = [0x21, 0, 0, 0, 0];
        (&mut write_data[1..5]).copy_from_slice(&address.to_be_bytes());

        let mut read_buffer = [0u8; 5];

        self.spi
            .transfer(&mut read_buffer, &write_data)
            .await
            .map_err(|e| e.kind())?;
        Ok(())
    }

    async fn erase_block_32kib_nb(&mut self, address: u32) -> Result<(), SpiFlashError> {
        self.enable_write().await?;

        let mut write_data: [u8; 5] = [0x52, 0, 0, 0, 0];
        (&mut write_data[1..5]).copy_from_slice(&address.to_be_bytes());

        let mut read_buffer = [0u8; 5];

        self.spi
            .transfer(&mut read_buffer, &write_data)
            .await
            .map_err(|e| e.kind())?;
        Ok(())
    }

    async fn erase_block_64kib_nb(&mut self, address: u32) -> Result<(), SpiFlashError> {
        self.enable_write().await?;

        let mut write_data: [u8; 5] = [0xDC, 0, 0, 0, 0];
        (&mut write_data[1..5]).copy_from_slice(&address.to_be_bytes());

        let mut read_buffer = [0u8; 5];

        self.spi
            .transfer(&mut read_buffer, &write_data)
            .await
            .map_err(|e| e.kind())?;

        Ok(())
    }

    // t_sus?
    async fn suspend_erase(&mut self) -> Result<(), SpiFlashError> {
        let write_data: [u8; 1] = [0x75];
        let mut read_buffer = [0u8; 1];
        self.spi
            .transfer(&mut read_buffer, &write_data)
            .await
            .map_err(|e| e.kind())?;

        Timer::after(Duration::from_micros(20)).await;

        Ok(())
    }

    async fn resume_erase(&mut self) -> Result<(), SpiFlashError> {
        let write_data: [u8; 1] = [0x7A];
        let mut read_buffer = [0u8; 1];
        self.spi
            .transfer(&mut read_buffer, &write_data)
            .await
            .map_err(|e| e.kind())?;

        Timer::after(Duration::from_micros(20)).await;

        Ok(())
    }

    async fn is_busy(&mut self) -> Result<bool, SpiFlashError> {
        let status_register_1 = self.read_status_register_1().await?;
        Ok(status_register_1.busy)
    }

    async fn wait_erase_finish(&mut self) -> Result<(), SpiFlashError> {
        self.wait_till_not_busy(Duration::from_millis(5), Duration::from_millis(2000))
            .await?;
        Ok(())
    }

    async fn read_4kib<'b>(
        &mut self,
        address: u32,
        read_length: usize,
        read_buffer: &'b mut [u8],
    ) -> Result<&'b [u8], SpiFlashError> {
        self.temp_buffer[0] = 0x13;
        (&mut self.temp_buffer[1..5]).copy_from_slice(&address.to_be_bytes());

        let first_5_bytes: [u8; 5] = read_buffer[0..5].try_into().unwrap();

        self.spi
            .transfer(
                &mut read_buffer[0..(read_length + 5)],
                &self.temp_buffer[0..(read_length + 5)],
            )
            .await
            .map_err(|e| e.kind())?;

        (&mut read_buffer[0..5]).copy_from_slice(&first_5_bytes);
        Ok(&read_buffer[5..(read_length + 5)])
    }

    async fn write_256b<'b>(
        &mut self,
        address: u32,
        write_buffer: &'b mut [u8],
    ) -> Result<(), SpiFlashError> {
        self.enable_write().await?;

        let first_5_bytes: [u8; 5] = write_buffer[0..5].try_into().unwrap();

        write_buffer[0] = 0x12;
        (&mut write_buffer[1..5]).copy_from_slice(&address.to_be_bytes());
        write_buffer[4] = 0;

        let transfer_len = min(261, write_buffer.len());
        self.spi
            .transfer(
                &mut self.temp_buffer[0..transfer_len],
                &write_buffer[0..transfer_len],
            )
            .await
            .map_err(|e| e.kind())?;

        (&mut write_buffer[0..5]).copy_from_slice(&first_5_bytes);

        self.wait_till_not_busy(Duration::from_micros(500), Duration::from_millis(4))
            .await?;
        Ok(())
    }
}
