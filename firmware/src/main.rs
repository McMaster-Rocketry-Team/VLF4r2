#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

// mod barometer;
mod barometer;
mod clock;
mod fmt;
mod gps;
mod h3lis100dl;
mod utils;

use crate::barometer::MS5607;
use crate::clock::Clock;
use crate::gps::{UartGPS, GPSPPS};
use crate::h3lis100dl::H3LIS100DL;
use defmt::info;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDeviceWithConfig;
use embassy_stm32::exti::{Channel, ExtiInput};
use embassy_stm32::gpio::Pin;
use embassy_stm32::spi::{Config as SpiConfig, Spi};
use embassy_stm32::time::Hertz;
use embassy_time::{Delay, Timer};
use embedded_hal_async::spi::SpiDevice;
use embedded_io_async::Write;
use firmware_common::driver::barometer::Barometer;
use firmware_common::driver::gps::{GPSParser, GPS};
use firmware_common::driver::imu::IMU;
use firmware_common::testMain;
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
use lora_phy::iv::GenericSx126xInterfaceVariant;
use lora_phy::mod_params::ModulationParams;
use lora_phy::mod_params::{Bandwidth, CodingRate, SpreadingFactor};
use lora_phy::mod_traits::{InterfaceVariant, RadioKind};
use lora_phy::sx126x::{self, Sx126x, Sx126xVariant, TcxoCtrlVoltage};
use lora_phy::{LoRa, RxMode};
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

    // let mut cs = Output::new(p.PD2, Level::High, Speed::High);

    let mut spi_config = SpiConfig::default();
    spi_config.frequency = Hertz(1_000_000);

    let mut spi1 = Mutex::<NoopRawMutex, _>::new(Spi::new(p.SPI1, p.PB3, p.PD7, p.PB4, p.DMA1_CH3, p.DMA1_CH2, spi_config));
    let h2lis100dl_spi_device = SpiDeviceWithConfig::new(
        &spi1,
        Output::new(p.PD2, Level::High, Speed::High),
        spi_config,
    );
    let mut h2lis100dl = H3LIS100DL::new(h2lis100dl_spi_device);
    h2lis100dl.reset().await.unwrap();
    loop {
        info!("{}",h2lis100dl.read().await.unwrap());
        sleep!(50);
    }
    // let tx_buffer = [0x0F | 0b10000000, 0x00];
    // let mut rx_buffer = [0u8; 2];
    // cs.set_low();
    // spi1.transfer(&mut rx_buffer, &tx_buffer).await.unwrap();
    // cs.set_high();
    // info!("{=u8:08b}", &rx_buffer[1]);

    // loop {
    //     sleep!(10);
    //     let tx_buffer = [0x2d | 0b10000000, 0x0];
    //     let mut rx_buffer = [0u8; 2];
    //     cs.set_low();
    //     spi1.transfer(&mut rx_buffer, &tx_buffer).await.unwrap();
    //     cs.set_high();
    //     info!("{=u8:08b}", &rx_buffer[1]);
    // }

    // let mut gps_pps = GPSPPS::new(p.PB5, p.EXTI5);
    // let mut gps = UartGPS::new(p.PB6, p.UART4, p.PA1, p.PA0, p.DMA1_CH5, p.DMA1_CH4);

    // let mut spi_config = SpiConfig::default();
    // spi_config.frequency = Hertz(250_000);
    // // should be spi6, but spi6 requires bdma which doesnt work
    // let spi1 = Mutex::<NoopRawMutex, _>::new(Spi::new(
    //     p.SPI1,
    //     p.PA5,
    //     p.PA7,
    //     p.PA6,
    //     p.DMA2_CH3,
    //     p.DMA2_CH2,
    //     spi_config.clone(),
    // ));
    // let lora_spi_device = SpiDeviceWithConfig::new(
    //     &spi1,
    //     Output::new(p.PE3, Level::High, Speed::High),
    //     spi_config,
    // );

    // let config = sx126x::Config {
    //     chip: Sx126xVariant::Sx1262,
    //     tcxo_ctrl: Some(TcxoCtrlVoltage::Ctrl1V7),
    //     use_dcdc: true,
    //     use_dio2_as_rfswitch: false,
    //     rx_boost: true,
    // };
    // let dio1 = Input::new(p.PC15.degrade(), Pull::Down);
    // let dio1 = ExtiInput::new(dio1, p.EXTI15.degrade());
    // let busy = Input::new(p.PC14.degrade(), Pull::Down);
    // let busy = ExtiInput::new(busy, p.EXTI14.degrade());
    // let iv = GenericSx126xInterfaceVariant::new(
    //     Output::new(p.PC13.degrade(), Level::High, Speed::Low),
    //     dio1,
    //     busy,
    //     Some(Output::new(p.PC0.degrade(), Level::High, Speed::High)),
    //     Some(Output::new(p.PC2.degrade(), Level::High, Speed::High)),
    // )
    // .unwrap();
    // let sx1262 = Sx126x::new(lora_spi_device, iv, config);
    // let mut lora = LoRa::new(sx1262, false, Delay).await.unwrap();

    // let modulation_params = lora
    //     .create_modulation_params(
    //         SpreadingFactor::_12,
    //         Bandwidth::_250KHz,
    //         CodingRate::_4_8,
    //         903_900_000,
    //     )
    //     .unwrap();
    // let mut tx_params = lora
    //     .create_tx_packet_params(4, false, false, false, &modulation_params)
    //     .unwrap();

    // info!("lora ready");
    // let buffer = "Hello VLF4".as_bytes();
    // loop {
    //     sleep!(5000);
    //     lora.prepare_for_tx(&modulation_params, &mut tx_params, 22, buffer)
    //         .await;
    //     lora.tx().await;
    //     lora.sleep(true).await;
    //     led2.set_high();
    //     sleep!(100);
    //     led2.set_low();
    // }

    // let rx_pkt_params = lora
    //     .create_rx_packet_params(4, false, 50, false, false, &modulation_params)
    //     .unwrap();
    // lora.prepare_for_rx(RxMode::Single(1000), &modulation_params, &rx_pkt_params)
    //     .await
    //     .unwrap();
    // info!("b");
    // let mut receiving_buffer = [00u8; 100];
    // let result = lora.rx(&rx_pkt_params, &mut receiving_buffer).await;
    // info!("lora init done");
    // loop {}

    // testMain(Clock::new(), gps, gps_pps, Delay).await;

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
