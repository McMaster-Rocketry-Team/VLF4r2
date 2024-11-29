use embassy_stm32::gpio::{AnyPin, Level, Output, Speed};
use firmware_common::driver::camera::Camera;

pub struct GPIOCamera {
    pin: Output<'static>,
}

impl GPIOCamera {
    pub fn new(pin: AnyPin) -> Self {
        Self {
            pin: Output::new(pin, Level::Low, Speed::Low),
        }
    }
}

impl Camera for GPIOCamera {
    type Error = ();

    async fn set_recording(&mut self, is_recording: bool) -> Result<(), Self::Error> {
        self.pin.set_level(if is_recording {
            Level::High
        } else {
            Level::Low
        });
        Ok(())
    }
}
