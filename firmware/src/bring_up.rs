#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::{gpio::Pin, time::mhz, Config};
use firmware_common::driver::indicator::Indicator;
use indicator::GPIOLED;
use panic_probe as _;
use defmt_rtt as _;

mod indicator;
mod utils;

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
        config.rcc.mux.fdcansel = Fdcansel::PLL1_Q;
        config.rcc.mux.usbsel = Usbsel::PLL3_Q;
    }
    let p = embassy_stm32::init(config);
    info!("Hello, World!");

    let mut led_blue = GPIOLED::new(p.PB12.degrade(), false);
    loop {
        info!("LED on");
        led_blue.set_enable(true).await;
        sleep!(500);
        info!("LED off");
        led_blue.set_enable(false).await;
        sleep!(500);
    }
}
