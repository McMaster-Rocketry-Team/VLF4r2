use defmt::trace;
use embassy_stm32::{
    bind_interrupts,
    exti::ExtiInput,
    gpio::{Level, Output, Speed},
    mode::Async,
    peripherals::{DMA2_CH0, DMA2_CH1, PA2, PA3, PE9, USART2},
    usart::{self, Config as UartConfig, Uart},
};
use embassy_time::Instant;
use firmware_common::driver::gps;
use heapless::String;

use crate::sleep;

bind_interrupts!(struct Irqs {
    USART2 => usart::InterruptHandler<USART2>;
});

pub struct UartGPS {
    uart: Option<Uart<'static, Async>>,
    buffer: [u8; 9],
    sentence: String<84>,
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
            uart: None,
            buffer: [0; 9],
            sentence: String::new(),
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
}

impl gps::GPS for UartGPS {
    async fn next_nmea_sentence(&mut self) -> gps::NmeaSentence {
        let uart = self.uart.as_mut().expect("GPS not initialized");
        'outer: loop {
            match uart.read_until_idle(&mut self.buffer).await {
                Ok(length) => {
                    for i in 0..length {
                        self.sentence.push(self.buffer[i] as char).ok();

                        if self.buffer[i] == 10u8 || self.sentence.len() == 84 {
                            if self.sentence.as_bytes()[0] != b'$' {
                                self.sentence.clear();
                            }

                            let sentence = self.sentence.clone();
                            self.sentence.clear();
                            for j in (i + 1)..length {
                                self.sentence.push(self.buffer[j] as char).ok();
                            }
                            break 'outer gps::NmeaSentence {
                                sentence,
                                timestamp: Instant::now().as_micros() as f64 / 1000.0,
                            };
                        }
                    }
                }
                Err(error) => {
                    defmt::warn!("Failed to read from GPS: {:?}", error);
                }
            }
        }
    }

    async fn reset(&mut self) {
        if let Some(uart) = self.uart.take() {
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

        self.uart.replace(uart);
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

impl gps::GPSPPS for GPSPPS {
    async fn wait_for_pps(&mut self) {
        self.pps.wait_for_rising_edge().await;
        trace!("PPS");
    }
}
