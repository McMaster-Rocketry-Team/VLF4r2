use defmt::unwrap;
use embassy_stm32::{bind_interrupts, peripherals, rng::{self, Rng}};
use firmware_common::driver::rng::RNG as RNGDriver;

bind_interrupts!(struct Irqs {
    RNG => rng::InterruptHandler<peripherals::RNG>;
});


pub struct RNG {
    rng: embassy_stm32::rng::Rng<'static, peripherals::RNG>,
}

impl RNG {
    pub fn new(rng: peripherals::RNG) -> Self {
        Self {
            rng: Rng::new(rng, Irqs),
        }
    }
}

impl RNGDriver for RNG {
    async fn next_bytes(&mut self, buffer: &mut [u8]) {
        unwrap!(self.rng.async_fill_bytes(buffer).await);
    }
}
