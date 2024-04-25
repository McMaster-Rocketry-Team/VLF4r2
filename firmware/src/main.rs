#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

// mod barometer;
mod barometer;
mod fmt;
mod gps;
mod shared_spi;
mod utils;

use crate::barometer::MS5607;
use crate::gps::UartGPS;
use defmt::info;
use embassy_stm32::exti::{Channel, ExtiInput};
use embedded_io_async::Write;
use firmware_common::driver::barometer::Barometer;
use firmware_common::driver::gps::{GPSParser, GPS};
use futures::join;

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::peripherals::{DMA1_CH4, DMA1_CH5, PA0, PA1, UART4};
use embassy_stm32::{
    bind_interrupts,
    gpio::{Input, Level, Output, Pull, Speed},
    spi,
    time::mhz,
    usart::{self, Config as UartConfig, Parity, Uart},
    Config,
};
use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    mutex::Mutex,
    pubsub::{PubSubBehavior, PubSubChannel},
};
#[cfg(not(debug_assertions))]
use panic_halt as _;
#[cfg(debug_assertions)]
use panic_probe as _;

const UPPER_ALTITUDE: f32 = 500.0;
const LOWER_ALTITUDE: f32 = 300.0; // ezmini: 396.24

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = Config::default();
    {
        use embassy_stm32::rcc::*;
        config.rcc.hsi = Some(HSIPrescaler::DIV1);
        config.rcc.csi = true;
        config.rcc.pll1 = Some(Pll {
            source: PllSource::HSI,
            prediv: PllPreDiv::DIV4,
            mul: PllMul::MUL50,
            divp: Some(PllDiv::DIV2),
            divq: Some(PllDiv::DIV8), // used by SPI3. 100Mhz.
            divr: None,
        });
        config.rcc.sys = Sysclk::PLL1_P; // 400 Mhz
        config.rcc.ahb_pre = AHBPrescaler::DIV2; // 200 Mhz
        config.rcc.apb1_pre = APBPrescaler::DIV2; // 100 Mhz
        config.rcc.apb2_pre = APBPrescaler::DIV2; // 100 Mhz
        config.rcc.apb3_pre = APBPrescaler::DIV2; // 100 Mhz
        config.rcc.apb4_pre = APBPrescaler::DIV2; // 100 Mhz
        config.rcc.voltage_scale = VoltageScale::Scale1;
    }
    let p = embassy_stm32::init(config);
    info!("Hello, World!");
    let mut led1 = Output::new(p.PE1, Level::Low, Speed::Low); // blue
    let mut led2 = Output::new(p.PE4, Level::Low, Speed::Low); // green
    let mut led3 = Output::new(p.PB9, Level::Low, Speed::Low); // red

    let mut gps_pps = Input::new(p.PB5, Pull::None);
    let mut gps_pps = ExtiInput::new(gps_pps, p.EXTI5);

    let mut gps = UartGPS::new(p.PB6, p.UART4, p.PA1, p.PA0, p.DMA1_CH5, p.DMA1_CH4);
    gps.reset().await;
    // // reset gps
    // {
    //     gps_nrst.set_low();
    //     sleep!(10);
    //     gps_nrst.set_high();
    //     sleep!(1000);

    //     let mut config = UartConfig::default();
    //     config.baudrate = 9600;
    //     let mut uart4 =
    //         Uart::new(p.UART4, p.PA1, p.PA0, Irqs, p.DMA1_CH4, p.DMA1_CH5, config).unwrap();
    //     uart4
    //         .write("$PMTK251,38400*27\r\n".as_bytes())
    //         .await
    //         .unwrap();
    //     sleep!(1000);
    //     drop(uart4);
    // }
    // let mut config = UartConfig::default();
    // config.baudrate = 38400;
    // let mut uart4 = unsafe {
    //     Uart::new(
    //         UART4::steal(),
    //         PA1::steal(),
    //         PA0::steal(),
    //         Irqs,
    //         DMA1_CH4::steal(),
    //         DMA1_CH5::steal(),
    //         config,
    //     )
    //     .unwrap()
    // };
    // uart4.write("$PMTK255,1*2D\r\n".as_bytes()).await.unwrap();

    // let gps_task = UartGPSTask::new();
    // let mut gps_task_ref = &gps_task;
    // let gpt_task_fut = gps_task.run(uart4);
    let parser = GPSParser::new();
    let gps_parser_fut = parser.run(&mut gps);

    let display_fut = async {
        loop {
            sleep!(500);
            let location = parser.get_nmea();
            info!("location: {:?}", location);
        }
    };

    let pps_fut = async {
        loop {
            gps_pps.wait_for_rising_edge().await;
            info!("PPS");
        }
    };

    join!(gps_parser_fut, display_fut, pps_fut);

    // let mut spi_config = spi::Config::default();
    // spi_config.frequency = mhz(1);

    // let baro_spi = spi::Spi::new(
    //     p.SPI2, p.PB10, p.PB15, p.PB14, p.DMA1_CH1, p.DMA1_CH0, spi_config,
    // );

    // let baro_spi = Mutex::<NoopRawMutex, _>::new(baro_spi);

    // // Baro
    // let mut barometer = MS5607::new(p.PA10, &baro_spi);

    // sleep!(500);
    // led1.set_low();
    // led2.set_low();
    // led3.set_low();

    // let altitude_channel = PubSubChannel::<NoopRawMutex, f32, 1, 3, 1>::new();

    // let baro_fut = async {
    //     let mut list = [0.0f32; 100];

    //     info!("reseting baro");
    //     barometer.reset().await.unwrap();
    //     let ground_altitude = barometer.read().await.unwrap().altitude();
    //     loop {
    //         let reading = barometer.read().await.unwrap();
    //         let altitude = reading.altitude() - ground_altitude;
    //         list.rotate_left(1);
    //         list[9] = altitude;
    //         let mut sum = 0.0;
    //         for i in 0..10 {
    //             sum += list[i];
    //         }
    //         let avg = sum / 10.0;
    //         altitude_channel.publish_immediate(avg);
    //     }
    // };

    // let drouge_chute_fut = async {
    //     let mut max = 0.0f32;
    //     let mut sub = altitude_channel.subscriber().unwrap();
    //     loop {
    //         let altitude = sub.next_message_pure().await;
    //         if altitude > max {
    //             max = altitude;
    //         }
    //         if altitude < max - 10.0 {
    //             sleep!(1000);
    //             led2.set_high();
    //             break;
    //         }
    //     }
    // };

    // let main_chute_fut = async {
    //     let mut past_upper = false;
    //     let mut deploy = false;
    //     let mut sub = altitude_channel.subscriber().unwrap();

    //     loop {
    //         let altitude = sub.next_message_pure().await;
    //         if altitude > UPPER_ALTITUDE {
    //             past_upper = true;
    //         }
    //         if altitude < LOWER_ALTITUDE && past_upper {
    //             deploy = true;
    //         }
    //         if deploy {
    //             led3.set_high();
    //         }
    //     }
    // };

    // let led_fut = async {
    //     let mut sub = altitude_channel.subscriber().unwrap();
    //     loop {
    //         let altitude = sub.next_message_pure().await;
    //         let mut light_state = floorf(altitude / 300.0) as i32;
    //         if light_state < 0 {
    //             light_state = 0;
    //         }

    //         info!("altitude: {:?}, light_state: {:?}", altitude, light_state);
    //         for _ in 0..=light_state {
    //             led1.set_high();
    //             sleep!(50);
    //             led1.set_low();
    //             sleep!(300);
    //         }

    //         sleep!(2000);
    //     }
    // };

    // join!(baro_fut, drouge_chute_fut, main_chute_fut, led_fut);
}
