use embassy_time::{Delay as EmbassyDelay, Instant};
use embedded_hal_async::delay::DelayNs;
use firmware_common::driver::delay::Delay as DelayTrait;

#[derive(Clone, Copy)]
pub struct Clock {}

impl Clock {
    pub fn new() -> Self {
        Self {}
    }
}

impl firmware_common::driver::clock::Clock for Clock {
    fn now_ms(&self) -> f64 {
        Instant::now().as_micros() as f64 / 1000.0
    }
}

#[derive(Clone, Copy)]
pub struct Delay;

impl DelayNs for Delay {
    async fn delay_ns(&mut self, ns: u32) {
        EmbassyDelay.delay_ns(ns).await
    }
}

impl DelayTrait for Delay {
    async fn delay_ms(&self, mut ms: f64) {
        EmbassyDelay.delay_ms(ms as u32).await;
        // while ms > 4294f64 {
        //     ms -= 4294f64;
        //     EmbassyDelay.delay_ns(4_294_000_000).await;
        // }
        // EmbassyDelay.delay_ns((ms * 1000_1000.0) as u32).await
    }
}
