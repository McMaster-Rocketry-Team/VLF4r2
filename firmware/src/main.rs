#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]
#![feature(try_blocks)]

// mod adc;
mod buzzer;
mod camera;
mod can_bus;
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
use buzzer::Buzzer;
use can_bus::CanBus;
use clock::{Clock, Delay};
use defmt::{debug, info};
use e22::E22;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDeviceWithConfig;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::Pin;
use embassy_stm32::i2c;
use embassy_stm32::i2c::Config as I2cConfig;
use embassy_stm32::i2c::I2c;
use embassy_stm32::pac;
use embassy_stm32::spi::{Config as SpiConfig, Spi};
use embassy_stm32::time::Hertz;
use firmware_common::driver::adc::DummyADC;
use firmware_common::driver::buzzer::Buzzer as _;
use heapless::Vec;

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
use firmware_common::driver::camera::DummyCamera;
use firmware_common::driver::debugger::DummyDebugger;
use firmware_common::driver::serial::get_dummy_serial;
use firmware_common::{vl_main, VLDeviceManager};
use futures::join;
use indicator::GPIOLED;
use lora_phy::iv::GenericSx126xInterfaceVariant;
use lora_phy::sx126x::{self, Sx126x, TcxoCtrlVoltage};
use lora_phy::LoRa;
// #[cfg(not(debug_assertions))]
// use panic_halt as _;
// #[cfg(debug_assertions)]
use embedded_alloc::Heap;
use panic_probe as _;
use pyro::{ArmingSwitch, PyroContinuity, PyroCtrl};
use rng::RNG;
use spi_flash::{CrcWrapper, SpiFlash};
use stm32_device_signature::device_id;
use sys_reset::SysReset;
use usb::Usb;
use vlfs::{EraseTune, ManagedEraseFlash};

bind_interrupts!(struct Irqs {
    I2C1_EV => i2c::EventInterruptHandler<I2C1>;
    I2C1_ER => i2c::ErrorInterruptHandler<I2C1>;
});

#[global_allocator]
static HEAP: Heap = Heap::empty();

#[link_section = ".ram_d3"]
static mut RAM_D3: [u8; 16 * 1024] = [0u8; 16 * 1024];

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = Config::default();
    {
        use embassy_stm32::rcc::mux::*;
        use embassy_stm32::rcc::*;

        config.rcc.hsi = Some(HSIPrescaler::DIV1);
        config.rcc.hse = Some(Hse {
            freq: mhz(16),
            mode: HseMode::Oscillator,
        });
        config.rcc.csi = false;
        config.rcc.hsi48 = Some(Hsi48Config {
            sync_from_usb: false,
        });
        config.rcc.ls = LsConfig::default_lsi();

        config.rcc.pll1 = Some(Pll {
            source: PllSource::HSE,
            prediv: PllPreDiv::DIV1,
            mul: PllMul::MUL32,
            divp: Some(PllDiv::DIV1),
            divq: Some(PllDiv::DIV4),
            divr: Some(PllDiv::DIV2),
        });

        config.rcc.pll2 = Some(Pll {
            source: PllSource::HSE,
            prediv: PllPreDiv::DIV1,
            mul: PllMul::MUL20,
            divp: Some(PllDiv::DIV8),
            divq: Some(PllDiv::DIV2),
            divr: Some(PllDiv::DIV2),
        });
        config.rcc.pll3 = Some(Pll {
            source: PllSource::HSE,
            prediv: PllPreDiv::DIV1,
            mul: PllMul::MUL24,
            divp: Some(PllDiv::DIV2),
            divq: Some(PllDiv::DIV8),
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

        config.rcc.mux.spi123sel = Saisel::PLL1_Q;
        config.rcc.mux.usart234578sel = Usart234578sel::PCLK1;
        config.rcc.mux.rngsel = Rngsel::HSI48;
        config.rcc.mux.i2c4sel = I2c4sel::PCLK4;
        config.rcc.mux.i2c1235sel = I2c1235sel::PCLK1;
        config.rcc.mux.spi6sel = Spi6sel::PCLK4;
        config.rcc.mux.spi45sel = Spi45sel::PCLK2;
        config.rcc.mux.adcsel = Adcsel::PLL2_P;
        config.rcc.mux.fdcansel = Fdcansel::PLL1_Q;
        config.rcc.mux.usbsel = Usbsel::PLL3_Q;
    }
    let p = embassy_stm32::init(config);
    info!("Hello, World!");
    let rcc_cr = pac::RCC.cr().read();
    
    let debug_fut = async {
        loop{
            sleep!(1000);
            debug!("HSE Ready: {} HSE on: {}", rcc_cr.hserdy(), rcc_cr.hseon());
        }
    };

    {
        use core::mem::MaybeUninit;
        const HEAP_SIZE: usize = 512;
        static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
        unsafe { HEAP.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE) }
    }

    let baro_buffer = unsafe { &mut RAM_D3[0..8] };

    let mut watchdog = IndependentWatchdog::new(p.IWDG1, 500_000);
    if !cfg!(debug_assertions) {
        watchdog.unleash();
    }
    let watchdog_fut = async {
        if !cfg!(debug_assertions) {
            loop {
                watchdog.pet();
                sleep!(250)
            }
        }
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

        // ADCs are broken on VLF4r2
        // debug!("Setting up ADCs");
        // let mut battery_adc = BatteryAdc::new(p.ADC1, p.PC0).await;
        // let mut current_adc = CurrentAdc::new(p.ADC2, p.ADC3, p.PC1, p.PC3).await;

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
        let mut gps = UartGPS::new(p.PE9, p.USART2, p.PA3, p.PA2, p.DMA2_CH0, p.DMA2_CH1);
        gps.reset().await;
        let gps_fut = gps.run(Clock::new());

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
        let lora = LoRa::new(sx1262, false, Delay).await.ok();
        // let lora: Option<LoRa<Sx126x<SpiDeviceWithConfig<NoopRawMutex, Spi<embassy_stm32::mode::Async>, Output>, GenericSx126xInterfaceVariant<Output, ExtiInput>, E22>, Delay>> = None;

        let rcc_cr = pac::RCC.cr().read();
        debug!("HSE Ready: {} HSE on: {}", rcc_cr.hserdy(), rcc_cr.hseon());
        // Rng
        debug!("Setting up RNG");
        let rng = RNG::new(p.RNG);

        // USB
        debug!("Setting up USB");
        let usb = Usb::new(p.USB_OTG_HS, p.PA12, p.PA11);
        let mut usb_runner = usb.get_runner();

        // Pyro
        debug!("Setting up Pyro");
        let arming_switch = ArmingSwitch::new(ExtiInput::new(p.PD11, p.EXTI11, Pull::Up));
        let pyro1_cont = PyroContinuity::new(ExtiInput::new(p.PC6, p.EXTI6, Pull::Up));
        let pyro1_ctrl = PyroCtrl::new(p.PC9.degrade());
        let pyro2_cont = PyroContinuity::new(ExtiInput::new(p.PA10, p.EXTI10, Pull::Up));
        let pyro2_ctrl = PyroCtrl::new(p.PC7.degrade());
        let pyro3_cont = PyroContinuity::new(ExtiInput::new(p.PA8, p.EXTI8, Pull::Up));
        let pyro3_ctrl = PyroCtrl::new(p.PD4.degrade());

        // Can bus
        debug!("Setting up CAN bus");
        let can_bus = CanBus::new(p.FDCAN1, p.PD0, p.PD1, p.PD3, p.PD5).await;

        // Camera
        // let camera = GPIOCamera::new(p.PD4.degrade());

        // System Reset
        debug!("Setting up System Reset");
        let sys_reset = SysReset::new();

        // loop {
        //     info!("Battery: {}V", battery_adc.read().await.unwrap());
        //     // battery_adc.read().await.unwrap();
        //     // current_adc.read().await.unwrap();
        //     sleep!(100);
        // }

        buzzer.play(3000, 50).await;

        let mut device_manager = VLDeviceManager::new(
            sys_reset,
            Clock::new(),
            Delay,
            flash,
            crc,
            low_g_imu,
            high_g_imu,
            DummyADC::new(Delay),
            DummyADC::new(Delay),
            (pyro1_cont, pyro1_ctrl),
            (pyro2_cont, pyro2_ctrl),
            (pyro3_cont, pyro3_ctrl),
            arming_switch,
            get_dummy_serial(Delay),
            &usb,
            buzzer,
            meg,
            lora,
            rng,
            led_red,
            led_green,
            led_blue,
            baro,
            &gps,
            gps_pps,
            DummyCamera {},
            can_bus,
            Vec::from_slice(&[0, 0]).unwrap(),
            DummyDebugger {},
        );

        info!("Starting firmware common");
        let firmware_common_future = vl_main(&mut device_manager, device_id(), None);

        #[allow(unreachable_code)]
        {
            join!(
                firmware_common_future, 
                usb_runner.run(), 
                gps_fut,
            );
        }
    };

    join!(main_fut, watchdog_fut, debug_fut);
}
