use embassy_stm32::{
    bind_interrupts,
    can::{self, frame::FdFrame, Can, CanConfigurator},
    gpio::{Input, Level, Output, Pull, Speed},
    peripherals::{FDCAN1, PD0, PD1, PD2, PD3, PD5},
};
use embedded_can::{ExtendedId, Id};
use firmware_common::driver::can_bus::CanBusTX;

use crate::sleep;

bind_interrupts!(struct Irqs {
    FDCAN1_IT0 => can::IT0InterruptHandler<FDCAN1>;
    FDCAN1_IT1 => can::IT1InterruptHandler<FDCAN1>;
});
pub struct CanBus {
    en: Output<'static>,
    stb_n: Output<'static>,
    err_n: Input<'static>,
    fdcan: Can<'static>,
}

impl CanBus {
    pub fn new(fdcan: FDCAN1, rx: PD0, tx: PD1, en: PD3, stb_n: PD5, err_n: PD2) -> Self {
        let mut can_config: CanConfigurator<_> = CanConfigurator::new(fdcan, rx, tx, Irqs);
        can_config.set_bitrate(250_000);
        can_config.set_fd_data_bitrate(250_000, false);
        let can = can_config.into_normal_mode();
        Self {
            en: Output::new(en, Level::Low, Speed::Low),
            stb_n: Output::new(stb_n, Level::Low, Speed::Low),
            err_n: Input::new(err_n, Pull::None),
            fdcan: can,
        }
    }
}

impl CanBusTX for CanBus {
    type Error = ();

    async fn reset(&mut self) -> Result<(), Self::Error> {
        self.en.set_low();
        self.stb_n.set_low();
        sleep!(10);
        self.en.set_high();
        self.stb_n.set_high();
        sleep!(10);
        Ok(())
    }

    async fn send(&mut self, raw_id: u32, data: &[u8]) -> Result<(), Self::Error> {
        let remote_frame = FdFrame::new_remote(Id::Extended(ExtendedId::new(raw_id).unwrap()), 10).unwrap();
        
        let frame = FdFrame::new_extended(raw_id, data).unwrap();
        self.fdcan.write_fd(&frame).await;
        if self.err_n.is_low() {
            defmt::warn!("CAN bus error")
        }
        Ok(())
    }
}
