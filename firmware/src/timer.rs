use embassy_time::Instant;
use vlfs::Timer as VLFSTimer;

#[derive(Clone, Copy)]
pub struct EmbassyTimer {}

impl EmbassyTimer {
    pub fn new() -> Self {
        Self {}
    }
}


impl VLFSTimer for EmbassyTimer {
    fn now_ms(&self) -> f64 {
        return Instant::now().as_micros() as f64 / 1000.0;
    }
}
