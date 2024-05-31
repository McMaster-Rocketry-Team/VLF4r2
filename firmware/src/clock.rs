use embassy_time::{Delay as EmbassyDelay, Instant};
use embedded_hal_async::delay::DelayNs;

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
