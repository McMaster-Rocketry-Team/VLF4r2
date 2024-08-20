use core::cell::RefCell;

use cortex_m::singleton;
use defmt::{trace, warn};
use embassy_stm32::{
    bind_interrupts,
    exti::ExtiInput,
    gpio::{Level, Output, Speed},
    peripherals::{DMA2_CH0, DMA2_CH1, PA2, PA3, PE9, USART2},
    usart::{self, BufferedUart, Config as UartConfig, Error, RingBufferedUartRx, Uart},
};
use embedded_io_async::{Read, Write};
use firmware_common::driver::uart_gps::UARTGPS as CommonUartGPS;
use firmware_common::{
    common::sensor_reading::SensorReading,
    driver::{clock::Clock, gps as common_gps, timestamp::BootTimestamp},
};

use crate::sleep;

bind_interrupts!(struct Irqs {
    USART2 => usart::BufferedInterruptHandler<USART2>;
});

pub struct UartGPS {
    uart: RefCell<BufferedUart<'static>>,
    gps: CommonUartGPS,
    nrst: Output<'static>,
}

impl UartGPS {
    pub fn new(
        nsrt: PE9,
        uart: USART2,
        rx: PA3,
        tx: PA2,
        _rx_dma: DMA2_CH0,
        _tx_dma: DMA2_CH1,
    ) -> Self {
        let tx_buf = singleton!(: [u8; 32] = [0; 32]).unwrap();
        let rx_buf = singleton!(: [u8; 32] = [0; 32]).unwrap();

        let mut config = UartConfig::default();
        config.baudrate = 9600;

        Self {
            uart: RefCell::new(
                BufferedUart::new(uart, Irqs, rx, tx, tx_buf, rx_buf, config).unwrap(),
            ),
            gps: CommonUartGPS::new(),
            nrst: Output::new(nsrt, Level::High, Speed::Low),
        }
    }

    pub async fn reset(&mut self) {
        self.nrst.set_low();
        sleep!(10);
        self.nrst.set_high();
        sleep!(1000);

        let mut uart = self.uart.borrow_mut();
        let mut config = UartConfig::default();
        config.baudrate = 9600;
        uart.set_config(&config).unwrap();

        uart.write("$PMTK251,38400*27\r\n".as_bytes())
            .await
            .unwrap();
        sleep!(1000);

        config.baudrate = 38400;
        uart.set_config(&config).unwrap();

        uart.write("$PMTK255,1*2D\r\n".as_bytes()).await.unwrap();
        sleep!(200);
    }

    pub async fn run(&self, clock: impl Clock) -> Result<(), Error> {
        let mut uart = self.uart.borrow_mut();
        self.gps.run(&mut *uart, clock).await;
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
