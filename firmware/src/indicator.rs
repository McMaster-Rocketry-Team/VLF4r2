use embassy_stm32::gpio::{AnyPin, Level, Output, Speed};
use firmware_common::driver::indicator::Indicator;

pub struct GPIOLED {
    output: Output<'static>,
    reversed: bool,
}

impl GPIOLED {
    pub fn new(pin: AnyPin, reversed: bool) -> Self {
        let output = Output::new(
            pin,
            if reversed { Level::High } else { Level::Low },
            Speed::Low,
        );
        Self { output, reversed }
    }
}

impl Indicator for GPIOLED {
    async fn set_enable(&mut self, enable: bool) {
        if enable {
            self.output.set_level(if self.reversed {
                Level::Low
            } else {
                Level::High
            });
        } else {
            self.output.set_level(if self.reversed {
                Level::High
            } else {
                Level::Low
            });
        }
    }
}
