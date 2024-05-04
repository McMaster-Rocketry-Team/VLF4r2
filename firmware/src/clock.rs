use embassy_time::Instant;

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
