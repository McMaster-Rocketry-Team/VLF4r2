use defmt::info;
use embassy_stm32::{
    adc::{Adc, Resolution, SampleTime, VrefInt},
    peripherals::{ADC1, ADC2, ADC3, PC0, PC1, PC3},
};
use embassy_time::{Duration, Instant, Timer};
use firmware_common::{
    common::sensor_reading::SensorReading,
    driver::{
        adc::{ADCData, Ampere, Volt, ADC},
        timestamp::BootTimestamp,
    },
};

// TODO temperature sensor

// vrefint: 1.216V

pub struct BatteryAdc {
    adc: Adc<'static, ADC1>,
    vrefint_channel: VrefInt,
    pin: PC0,
}

impl BatteryAdc {
    pub async fn new(adc: ADC1, pin: PC0) -> Self {
        let mut adc = Adc::new(adc);
        adc.set_resolution(Resolution::BITS16);
        adc.set_sample_time(SampleTime::CYCLES810_5);
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

impl ADC<Volt> for BatteryAdc {
    type Error = ();

    // unit: V
    async fn read(&mut self) -> Result<SensorReading<BootTimestamp, ADCData<Volt>>, Self::Error> {
        let vrefint = self.adc.read(&mut self.vrefint_channel);
        info!("Vref 1: {}", vrefint);
        let vref_ratio = 1.216 / vrefint as f32;
        let measured = self.adc.read(&mut self.pin);
        info!("measured: {}", measured);
        let divider_ratio = 0.163f32;

        Ok(SensorReading::new(
            Instant::now().as_micros() as f64 / 1000.0,
            ADCData::new(measured as f32 * vref_ratio / divider_ratio),
        ))
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
        adc2.set_resolution(Resolution::BITS16);
        adc2.set_sample_time(SampleTime::CYCLES810_5);
        let adc2_vrefint = adc2.enable_vrefint();

        let mut adc3 = Adc::new(adc3);
        adc3.set_resolution(Resolution::BITS16);
        adc3.set_sample_time(SampleTime::CYCLES810_5);
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

impl ADC<Ampere> for CurrentAdc {
    type Error = ();

    // unit: A
    async fn read(&mut self) -> Result<SensorReading<BootTimestamp, ADCData<Ampere>>, Self::Error> {
        // TODO
        let vrefint2 = self.adc2.read(&mut self.adc2_vrefint);
        info!("Vref 2: {}", vrefint2);
        // let vrefint3 = self.adc3.read(&mut self.adc3_vrefint);
        // info!("Vref 3: {}", vrefint3);

        Ok(SensorReading::new(
            Instant::now().as_micros() as f64 / 1000.0,
            ADCData::new(0.0),
        ))
    }
}
