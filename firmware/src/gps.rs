use core::cell::RefCell;

use defmt::{trace, warn};
use embassy_stm32::{
    bind_interrupts,
    exti::ExtiInput,
    gpio::{Level, Output, Speed},
    mode::Async,
    peripherals::{DMA2_CH0, DMA2_CH1, PA2, PA3, PE9, USART2},
    usart::{self, Config as UartConfig, Error, RingBufferedUartRx, Uart},
};
use embedded_io_async::Read;
use firmware_common::driver::uart_gps::UARTGPS as CommonUartGPS;
use firmware_common::{
    common::sensor_reading::SensorReading,
    driver::{clock::Clock, gps as common_gps, timestamp::BootTimestamp},
};

use crate::sleep;

bind_interrupts!(struct Irqs {
    USART2 => usart::InterruptHandler<USART2>;
});

pub struct UartGPS {
    uart: RefCell<Option<Uart<'static, Async>>>,
    gps: CommonUartGPS,
    nrst: Output<'static>,
}

impl UartGPS {
    pub fn new(
        nsrt: PE9,
        _uart: USART2,
        _rx: PA3,
        _tx: PA2,
        _rx_dma: DMA2_CH0,
        _tx_dma: DMA2_CH1,
    ) -> Self {
        Self {
            uart: RefCell::new(None),
            gps: CommonUartGPS::new(),
            nrst: Output::new(nsrt, Level::High, Speed::Low),
        }
    }

    fn create_uart(baudrate: u32) -> Uart<'static, Async> {
        let mut config = UartConfig::default();
        config.baudrate = baudrate;

        // Safety: peripherals stealed here are all claimed by the constructor.
        unsafe {
            Uart::new(
                USART2::steal(),
                PA3::steal(),
                PA2::steal(),
                Irqs,
                DMA2_CH1::steal(),
                DMA2_CH0::steal(),
                config,
            )
            .unwrap()
        }
    }

    pub async fn reset(&mut self) {
        if let Some(uart) = self.uart.borrow_mut().take() {
            drop(uart);
        }

        self.nrst.set_low();
        sleep!(10);
        self.nrst.set_high();
        sleep!(1000);

        let mut uart = Self::create_uart(9600);
        uart.write("$PMTK251,38400*27\r\n".as_bytes())
            .await
            .unwrap();
        sleep!(1000);
        drop(uart);

        let mut uart = Self::create_uart(38400);
        uart.write("$PMTK255,1*2D\r\n".as_bytes()).await.unwrap();
        sleep!(200);

        self.uart.replace(Some(uart));
    }

    pub async fn run(&self, clock: impl Clock) -> Result<(), Error> {
        let uart = self.uart.borrow_mut().take().unwrap();
        let rx = uart.split().1;
        let mut buffer = [0u8; 84];
        let rx = rx.into_ring_buffered(&mut buffer);
        let mut rx = RingBufferedRxOverrunWrapper(rx);
        self.gps.run(&mut rx, clock).await;
    }
}

struct RingBufferedRxOverrunWrapper<'a>(RingBufferedUartRx<'a>);

impl<'a> embedded_io_async::ErrorType for RingBufferedRxOverrunWrapper<'a> {
    type Error = Error;
}

impl<'a> embedded_io_async::Read for RingBufferedRxOverrunWrapper<'a> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let result = self.0.read(buf).await;
        match result {
            Ok(size) => Ok(size),
            Err(Error::Overrun) => {
                warn!("Uart overrun");
                self.0.start()?;
                Err(Error::Overrun)
            }
            Err(e) => Err(e),
        }
    }
}

impl common_gps::GPS for &UartGPS {
    type Error = Error;

    async fn next_location(
        &mut self,
    ) -> Result<SensorReading<BootTimestamp, common_gps::GPSData>, Self::Error> {
        Ok((&self.gps).next_location().await.unwrap())
    }
}

pub struct GPSPPS {
    pps: ExtiInput<'static>,
}

impl GPSPPS {
    pub fn new(pps: ExtiInput<'static>) -> Self {
        Self { pps }
    }
}

impl common_gps::GPSPPS for GPSPPS {
    async fn wait_for_pps(&mut self) {
        self.pps.wait_for_rising_edge().await;
        trace!("PPS");
    }
}
