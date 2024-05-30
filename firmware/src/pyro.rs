use embassy_stm32::{
    exti::ExtiInput,
    gpio::{AnyPin, Level, Output, Speed},
};
use firmware_common::driver::arming::HardwareArming;
use firmware_common::driver::pyro::{Continuity, PyroCtrl as PyroCtrlDriver};

pub struct PyroContinuity {
    continuity_input: ExtiInput<'static>,
}

impl PyroContinuity {
    pub fn new(continuity_input: ExtiInput<'static>) -> Self {
        Self { continuity_input }
    }
}

impl Continuity for PyroContinuity {
    type Error = ();

    async fn wait_continuity_change(&mut self) -> Result<bool, ()> {
        self.continuity_input.wait_for_any_edge().await;
        Ok(self.continuity_input.is_low())
    }

    async fn read_continuity(&mut self) -> Result<bool, ()> {
        Ok(self.continuity_input.is_low())
    }
}


pub struct PyroCtrl {
    ctrl_pin: Output<'static>,
}

impl PyroCtrl {
    pub fn new(ctrl_pin: AnyPin) -> Self {
        Self { ctrl_pin:Output::new(ctrl_pin, Level::Low, Speed::Low) }
    }
}

impl PyroCtrlDriver for PyroCtrl {
    type Error = ();

    async fn set_enable(&mut self, enable: bool) -> Result<(), Self::Error> {
        self.ctrl_pin
            .set_level(if enable { Level::High } else { Level::Low });
        Ok(())
    }
}



pub struct ArmingSwitch {
    switch: ExtiInput<'static>,
}

impl ArmingSwitch {
    pub fn new(switch: ExtiInput<'static>) -> Self {
        Self { switch }
    }
}

impl HardwareArming for ArmingSwitch {
    type Error = ();

    async fn wait_arming_change(&mut self) -> Result<bool, ()> {
        self.switch.wait_for_any_edge().await;
        Ok(self.switch.is_low())
    }

    async fn read_arming(&mut self) -> Result<bool, ()> {
        Ok(self.switch.is_low())
    }
}
