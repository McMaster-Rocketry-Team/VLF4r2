use embassy_stm32::{
    adc::{Adc, SampleTime, VrefInt},
    peripherals::{ADC1, ADC2, ADC3, PC0, PC1, PC3},
};
use embassy_time::{Duration, Timer};
use firmware_common::driver::adc::ADC;

// TODO temperature sensor

pub struct BatteryAdc {
    adc: Adc<'static, ADC1>,
    vrefint_channel: VrefInt,
    pin: PC0,
}

impl BatteryAdc {
    pub async fn new(adc: ADC1, pin: PC0) -> Self {
        let mut adc = Adc::new(adc);
        adc.set_sample_time(SampleTime::CYCLES32_5);
        let vrefint_channel = adc.enable_vrefint();

        // wait for the internal voltage to stabilize
        Timer::after(Duration::from_micros(10)).await;

        Self {
            adc,
            vrefint_channel,
            pin,
        }
    }
}

impl ADC for BatteryAdc {
    type Error = ();

    // unit: V
    async fn read(&mut self) -> Result<f32, Self::Error> {
        // TODO check conversion
        let measured = self.adc.read(&mut self.pin);
        Ok(measured as f32 * 6f32 / 1000f32)
    }
}

pub struct CurrentAdc {
    adc2: Adc<'static, ADC2>,
    adc2_vrefint: VrefInt,
    adc2_pin_curr_ref: PC1,
    adc3: Adc<'static, ADC3>,
    adc3_vrefint: VrefInt,
    adc3_pin_curr_m: PC3,
}


impl CurrentAdc {
    pub async fn new(adc2: ADC2, adc3: ADC3, adc2_pin_curr_ref: PC1, adc3_pin_curr_m: PC3) -> Self {
        let mut adc2 = Adc::new(adc2);
        adc2.set_sample_time(SampleTime::CYCLES32_5);
        let adc2_vrefint = adc2.enable_vrefint();

        let mut adc3 = Adc::new(adc3);
        adc3.set_sample_time(SampleTime::CYCLES32_5);
        let adc3_vrefint = adc3.enable_vrefint();

        // wait for the internal voltage to stabilize
        Timer::after(Duration::from_micros(10)).await;

        Self {
            adc2,
            adc2_vrefint,
            adc2_pin_curr_ref,
            adc3,
            adc3_vrefint,
            adc3_pin_curr_m,
        }
    }
}

impl ADC for CurrentAdc {
    type Error = ();

    // unit: A
    async fn read(&mut self) -> Result<f32, Self::Error> {
        // TODO
        Ok(0.0)
    }
}