#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]

mod adc;
mod buzzer;
mod clock;
mod e22;
mod fmt;
mod gps;
mod h3lis100dl;
mod indicator;
mod lsm6dsm;
mod mmc5603;
mod ms5607;
mod pyro;
mod rng;
mod spi_flash;
mod sys_reset;
mod timer;
mod usb;
mod utils;

use crate::gps::{UartGPS, GPSPPS};
use crate::h3lis100dl::H3LIS100DL;
use crate::lsm6dsm::LSM6DSM;
use crate::mmc5603::MMC5603;
use crate::ms5607::MS5607;
use adc::{BatteryAdc, CurrentAdc};
use buzzer::Buzzer;
use clock::{Clock, Delay};
use defmt::{debug, info};
use e22::E22;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDeviceWithConfig;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::Pin;
use embassy_stm32::i2c;
use embassy_stm32::i2c::Config as I2cConfig;
use embassy_stm32::i2c::I2c;
use embassy_stm32::spi::{Config as SpiConfig, Spi};
use embassy_stm32::time::Hertz;

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::peripherals::I2C1;
use embassy_stm32::wdg::IndependentWatchdog;
use embassy_stm32::{
    bind_interrupts,
    gpio::{Level, Output, Pull, Speed},
    time::mhz,
    Config,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use firmware_common::driver::barometer::Barometer;
use firmware_common::driver::buzzer::Buzzer as _;
use firmware_common::driver::camera::DummyCamera;
use firmware_common::driver::debugger::DummyDebugger;
use firmware_common::driver::serial::DummySerial;
use firmware_common::driver::usb::DummyUSB;
use firmware_common::{init, DeviceManager};
use futures::join;
use indicator::GPIOLED;
use lora_phy::iv::GenericSx126xInterfaceVariant;
use lora_phy::sx126x::{self, Sx126x, TcxoCtrlVoltage};
use lora_phy::LoRa;
// #[cfg(not(debug_assertions))]
// use panic_halt as _;
// #[cfg(debug_assertions)]
use panic_probe as _;
use pyro::{ArmingSwitch, PyroContinuity, PyroCtrl};
use rng::RNG;
use spi_flash::{CrcWrapper, SpiFlash};
use sys_reset::SysReset;
use usb::Usb;
use vlfs::{EraseTune, ManagedEraseFlash};

// bind_interrupts!(struct Irqs {
//     FDCAN2_IT0 => can::IT0InterruptHandler<FDCAN2>;
//     FDCAN2_IT1 => can::IT1InterruptHandler<FDCAN2>;
// });

bind_interrupts!(struct Irqs {
    I2C1_EV => i2c::EventInterruptHandler<I2C1>;
    I2C1_ER => i2c::ErrorInterruptHandler<I2C1>;
});

#[link_section = ".ram_d3"]
static mut RAM_D3: [u8; 16 * 1024] = [0u8; 16 * 1024];

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = Config::default();
    {
        use embassy_stm32::rcc::mux::*;
        use embassy_stm32::rcc::*;

        config.rcc.hsi = None;
        config.rcc.hse = Some(Hse {
            freq: mhz(16),
            mode: HseMode::Oscillator,
        });
        config.rcc.csi = true;
        config.rcc.hsi48 = None;
        config.rcc.ls = LsConfig::default_lsi();

        config.rcc.pll1 = Some(Pll {
            source: PllSource::HSE,
            prediv: PllPreDiv::DIV1,
            mul: PllMul::MUL32,
            divp: Some(PllDiv::DIV1),
            divq: Some(PllDiv::DIV64),
            divr: Some(PllDiv::DIV5),
        });

        config.rcc.pll2 = Some(Pll {
            source: PllSource::HSE,
            prediv: PllPreDiv::DIV1,
            mul: PllMul::MUL12,
            divp: Some(PllDiv::DIV2),
            divq: Some(PllDiv::DIV128),
            divr: Some(PllDiv::DIV2),
        });
        config.rcc.pll3 = Some(Pll {
            source: PllSource::HSE,
            prediv: PllPreDiv::DIV1,
            mul: PllMul::MUL12,
            divp: Some(PllDiv::DIV2),
            divq: Some(PllDiv::DIV2),
            divr: Some(PllDiv::DIV2),
        });

        config.rcc.sys = Sysclk::PLL1_P;
        config.rcc.d1c_pre = AHBPrescaler::DIV1;

        config.rcc.apb1_pre = APBPrescaler::DIV2;
        config.rcc.apb2_pre = APBPrescaler::DIV2;
        config.rcc.apb3_pre = APBPrescaler::DIV2;
        config.rcc.apb4_pre = APBPrescaler::DIV2;
        config.rcc.ahb_pre = AHBPrescaler::DIV2;

        config.rcc.voltage_scale = VoltageScale::Scale0;

        config.rcc.mux.spi123sel = Saisel::PLL2_P;
        config.rcc.mux.usart16910sel = Usart16910sel::PLL3_Q;
        config.rcc.mux.usart234578sel = Usart234578sel::PLL3_Q;
        config.rcc.mux.i2c1235sel = I2c1235sel::PLL3_R;
        config.rcc.mux.i2c4sel = I2c4sel::PLL3_R;
        config.rcc.mux.adcsel = Adcsel::PLL3_R;
        config.rcc.mux.fdcansel = Fdcansel::PLL2_Q;
        config.rcc.mux.usbsel = Usbsel::PLL3_Q;
        config.rcc.mux.rngsel = Rngsel::PLL1_Q;
    }
    let p = embassy_stm32::init(config);
    info!("Hello, World!");
    

    let baro_buffer = unsafe { &mut RAM_D3[0..8] };

    let mut watchdog = IndependentWatchdog::new(p.IWDG1, 500_000);
    // watchdog.unleash();
    let watchdog_fut = async {
        // loop {
        //     watchdog.pet();
        //     sleep!(250)
        // }
    };


    let main_fut = async {
        let led_blue = GPIOLED::new(p.PB12.degrade(), false);
        let led_green = GPIOLED::new(p.PC4.degrade(), false);
        let led_red = GPIOLED::new(p.PD10.degrade(), false);
        let en_5v = Output::new(p.PE3, Level::High, Speed::Low);

        // baro
        debug!("Setting up Barometer");
        let mut spi_config = SpiConfig::default();
        spi_config.frequency = Hertz(250_000);
        let spi6 = Mutex::<NoopRawMutex, _>::new(Spi::new(
            p.SPI6, p.PB3, p.PB5, p.PB4, p.BDMA_CH1, p.BDMA_CH0, spi_config,
        ));
        let baro_spi_device = SpiDeviceWithConfig::new(
            &spi6,
            Output::new(p.PC14, Level::High, Speed::High),
            spi_config,
        );
        let mut baro = MS5607::new(baro_spi_device, baro_buffer);
        debug!("resetting");
        baro.reset().await.unwrap();

        // flash
        debug!("Setting up Flash");
        let mut spi_config = SpiConfig::default();
        spi_config.frequency = Hertz(1_000_000);
        let spi4 = Mutex::<NoopRawMutex, _>::new(Spi::new(
            p.SPI4, p.PE12, p.PE14, p.PE13, p.DMA1_CH1, p.DMA1_CH0, spi_config,
        ));

        let flash_spi_device = SpiDeviceWithConfig::new(
            &spi4,
            Output::new(p.PE15, Level::High, Speed::High),
            spi_config,
        );

        let flash_n_reset = Output::new(p.PD9, Level::Low, Speed::Low);
        let flash = SpiFlash::new(flash_spi_device, flash_n_reset);
        let flash = ManagedEraseFlash::new(
            flash,
            Delay,
            EraseTune {
                erase_us_every_write_256b: 1000,
            },
        );

        // CRC
        debug!("Setting up CRC");
        let crc = CrcWrapper::new(p.CRC);

        // Low G IMU
        debug!("Setting up Low G IMU");
        let mut spi_config = SpiConfig::default();
        spi_config.frequency = Hertz(1_000_000);
        let spi3 = Mutex::<NoopRawMutex, _>::new(Spi::new(
            p.SPI3, p.PC10, p.PC12, p.PC11, p.DMA1_CH4, p.DMA1_CH5, spi_config,
        ));
        let low_g_imu_spi_device = SpiDeviceWithConfig::new(
            &spi3,
            Output::new(p.PC13, Level::High, Speed::High),
            spi_config,
        );
        let low_g_imu = LSM6DSM::new(low_g_imu_spi_device);

        // High G IMU
        debug!("Setting up High G IMU");
        let mut spi_config = SpiConfig::default();
        spi_config.frequency = Hertz(1_000_000);
        let spi1 = Mutex::<NoopRawMutex, _>::new(Spi::new(
            p.SPI1, p.PA5, p.PA7, p.PA6, p.DMA1_CH3, p.DMA1_CH2, spi_config,
        ));
        let high_g_imu_spi_device = SpiDeviceWithConfig::new(
            &spi1,
            Output::new(p.PA1, Level::High, Speed::High),
            spi_config,
        );
        let high_g_imu = H3LIS100DL::new(high_g_imu_spi_device);

        // ADC
        debug!("Setting up ADCs");
        let battery_adc = BatteryAdc::new(p.ADC1, p.PC0).await;
        let current_adc = CurrentAdc::new(p.ADC2, p.ADC3, p.PC1, p.PC3).await;

        // meg
        debug!("Setting up Megnetometer");
        let mut i2c_config = I2cConfig::default();
        i2c_config.sda_pullup = true;
        i2c_config.scl_pullup = true;
        let i2c1 = I2c::new(
            p.I2C1,
            p.PB8,
            p.PB9,
            Irqs,
            p.DMA1_CH7,
            p.DMA1_CH6,
            Hertz(100_000),
            i2c_config,
        );
        let meg = MMC5603::new(i2c1);

        // GPS
        debug!("Setting up GPS");
        let gps_pps = GPSPPS::new(ExtiInput::new(p.PD12, p.EXTI12, Pull::None));
        let gps = UartGPS::new(p.PE9, p.USART2, p.PA3, p.PA2, p.DMA2_CH0, p.DMA2_CH1);

        // Buzzer
        debug!("Setting up Buzzer");
        let mut buzzer = Buzzer::new(p.TIM2, p.PA0);

        // lora
        debug!("Setting up LoRa");
        let mut spi_config = SpiConfig::default();
        spi_config.frequency = Hertz(250_000);
        let spi2 = Mutex::<NoopRawMutex, _>::new(Spi::new(
            p.SPI2, p.PB13, p.PB15, p.PB14, p.DMA2_CH3, p.DMA2_CH2, spi_config,
        ));

        let lora_spi_device = SpiDeviceWithConfig::new(
            &spi2,
            Output::new(p.PB0, Level::High, Speed::High),
            spi_config,
        );

        let config = sx126x::Config {
            chip: E22,
            tcxo_ctrl: Some(TcxoCtrlVoltage::Ctrl1V7),
            use_dcdc: true,
            rx_boost: true,
        };
        let iv = GenericSx126xInterfaceVariant::new(
            Output::new(p.PE8.degrade(), Level::High, Speed::Low),
            ExtiInput::new(p.PD15, p.EXTI15, Pull::Down),
            ExtiInput::new(p.PD13, p.EXTI13, Pull::Down),
            Some(Output::new(p.PC5.degrade(), Level::High, Speed::High)),
            Some(Output::new(p.PD14.degrade(), Level::High, Speed::High)),
        )
        .unwrap();
        let sx1262 = Sx126x::new(lora_spi_device, iv, config);
        let lora = LoRa::new(sx1262, false, Delay).await.unwrap();

        // Rng
        debug!("Setting up RNG");
        let rng = RNG::new(p.RNG);

        // USB
        // debug!("Setting up USB");
        // let (usb, mut usb_runner) = Usb::new(p.USB_OTG_HS, p.PA12, p.PA11);

        // Pyro
        debug!("Setting up Pyro");
        let arming_switch = ArmingSwitch::new(ExtiInput::new(p.PD11, p.EXTI11, Pull::Up));
        let pyro1_cont = PyroContinuity::new(ExtiInput::new(p.PC6, p.EXTI6, Pull::Up));
        let pyro1_ctrl = PyroCtrl::new(p.PC9.degrade());
        let pyro2_cont = PyroContinuity::new(ExtiInput::new(p.PA10, p.EXTI10, Pull::Up));
        let pyro2_ctrl = PyroCtrl::new(p.PC7.degrade());
        let pyro3_cont = PyroContinuity::new(ExtiInput::new(p.PA8, p.EXTI8, Pull::Up));
        let pyro3_ctrl = PyroCtrl::new(p.PD4.degrade());

        // System Reset
        let sys_reset = SysReset::new();

        buzzer.play(3000, 50).await;

        let mut device_manager = DeviceManager::new(
            sys_reset,
            Clock::new(),
            Delay,
            flash,
            crc,
            low_g_imu,
            battery_adc,
            current_adc,
            (pyro1_cont, pyro1_ctrl),
            (pyro2_cont, pyro2_ctrl),
            (pyro3_cont, pyro3_ctrl),
            arming_switch,
            DummySerial::new(Delay),
            DummyUSB::new(Delay),
            buzzer,
            meg,
            lora,
            rng,
            led_green,
            led_red,
            baro,
            gps,
            gps_pps,
            DummyCamera {},
            DummyDebugger {},
        );

        info!("Starting firmware common");
        let firmware_common_future = init(
            &mut device_manager,
            Some(firmware_common::DeviceMode::GroundTestGCM),
        );

        #[allow(unreachable_code)]
        {
            join!(
                firmware_common_future, 
                // usb_runner.run(),
            );
        }
    };

    join!(main_fut, watchdog_fut);

    // let mut can_en = Output::new(p.PB11, Level::Low, Speed::Low);
    // let mut can_stb_n = Output::new(p.PE12, Level::Low, Speed::Low);
    // let mut can_err_n = Input::new(p.PD4, Pull::None);

    // sleep!(10);
    // can_en.set_high();
    // can_stb_n.set_high();

    // let mut can = can::CanConfigurator::new(p.FDCAN2, p.PB12, p.PB13, Irqs);
    // can.set_bitrate(250_000);
    // let can = can.into_normal_mode();
    // info!("CAN Configured");

    // let (mut tx, mut rx, _props) = can.split();

    // let send_fut = async {
    //     sleep!(5000);
    //     loop {
    //         sleep!(500);
    //         let frame = can::frame::Frame::new_extended(10, &[44; 8]).unwrap();

    //         info!("write");
    //         tx.write(&frame).await;
    //         sleep!(1);
    //     }
    // };

    // let read_fut = async {
    //     loop {
    //         match rx.read().await {
    //             Ok(_envelope) => {
    //                 led1.set_high();
    //                 sleep!(100);
    //                 led1.set_low();
    //             }
    //             Err(err) => {
    //                 error!("Error in frame {}", err);
    //                 // panic!();
    //             },
    //         }
    //     }
    // };
    // join!(send_fut, read_fut);

    // loop {
    //     sleep!(10);
    //     let tx_buffer = [0x2d | 0b10000000, 0x0];
    //     let mut rx_buffer = [0u8; 2];
    //     cs.set_low();
    //     spi1.transfer(&mut rx_buffer, &tx_buffer).await.unwrap();
    //     cs.set_high();
    //     info!("{=u8:08b}", &rx_buffer[1]);
    // }

    // info!("a");

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
