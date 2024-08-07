use embassy_stm32::{
    bind_interrupts,
    can::{self, enums::BusError, frame::Envelope, Can, CanConfigurator, CanRx, CanTx, Frame},
    gpio::{Input, Level, Output, Pull, Speed},
    peripherals::{FDCAN1, PD0, PD1, PD2, PD3, PD5},
};
use embedded_can::{ExtendedId, Id};
use firmware_common::{
    common::can_bus::message::CanBusMessage,
    driver::can_bus::{
        CanBusRX as CanBusRXTrait, CanBusRawMessage, CanBusTX as CanBusTXTrait, SplitableCanBus,
    },
};

use crate::sleep;

bind_interrupts!(struct Irqs {
    FDCAN1_IT0 => can::IT0InterruptHandler<FDCAN1>;
    FDCAN1_IT1 => can::IT1InterruptHandler<FDCAN1>;
});

pub struct CanBus {
    fdcan: Can<'static>,
    en: Output<'static>,
    stb_n: Output<'static>,
    err_n: Input<'static>,
    self_node_type: u8,
    self_node_id: u16,
}

impl CanBus {
    pub fn new(fdcan: FDCAN1, rx: PD0, tx: PD1, en: PD3, stb_n: PD5, err_n: PD2) -> Self {
        let mut can_config: CanConfigurator = CanConfigurator::new(fdcan, rx, tx, Irqs);
        can_config.set_bitrate(250_000);
        let can = can_config.into_normal_mode();

        Self {
            fdcan: can,
            en: Output::new(en, Level::Low, Speed::Low),
            stb_n: Output::new(stb_n, Level::Low, Speed::Low),
            err_n: Input::new(err_n, Pull::None),
            self_node_type: 0,
            self_node_id: 0,
        }
    }
}

impl SplitableCanBus for CanBus {
    type Error = BusError;
    type TX = CanBusTx;
    type RX = CanBusRx;

    async fn reset(&mut self) -> Result<(), Self::Error> {
        self.en.set_low();
        self.stb_n.set_low();
        sleep!(10);
        self.en.set_high();
        self.stb_n.set_high();
        sleep!(10);
        Ok(())
    }

    fn configure_self_node(&mut self, node_type: u8, node_id: u16) {
        self.self_node_type = node_type;
        self.self_node_id = node_id;
    }

    fn split(self) -> (CanBusTx, CanBusRx) {
        let (can_tx, can_rx, _) = self.fdcan.split();
        (
            CanBusTx {
                can_tx,
                self_node_id: self.self_node_id,
                self_node_type: self.self_node_type,
            },
            CanBusRx { can_rx },
        )
    }
}

pub struct CanBusTx {
    self_node_type: u8,
    self_node_id: u16,
    can_tx: CanTx<'static>,
}

impl CanBusTXTrait for CanBusTx {
    type Error = BusError;

    async fn send<T: CanBusMessage>(
        &mut self,
        message: &T,
        priority: u8,
    ) -> Result<(), Self::Error> {
        let frame = Frame::new_extended(
            T::create_id(priority, self.self_node_type, self.self_node_id).into(),
            &message.to_data(),
        )
        .unwrap();
        if let Some(returned_frame) = self.can_tx.write(&frame).await {
            self.can_tx.write(&returned_frame).await;
        }
        Ok(())
    }

    async fn send_remote<T: CanBusMessage>(&mut self, priority: u8) -> Result<(), Self::Error> {
        let frame = Frame::new_remote(
            Id::Extended(
                ExtendedId::new(
                    T::create_id(priority, self.self_node_type, self.self_node_id).into(),
                )
                .unwrap(),
            ),
            T::len(),
        )
        .unwrap();
        if let Some(returned_frame) = self.can_tx.write(&frame).await {
            self.can_tx.write(&returned_frame).await;
        }
        Ok(())
    }
}

pub struct EnvelopeWrapper(Envelope);

impl CanBusRawMessage for EnvelopeWrapper {
    fn id(&self) -> u32 {
        match self.0.frame.id() {
            Id::Standard(id) => id.as_raw() as u32,
            Id::Extended(id) => id.as_raw(),
        }
    }

    fn rtr(&self) -> bool {
        self.0.frame.header().rtr()
    }

    fn data(&self) -> &[u8] {
        self.0.frame.data()
    }
}

pub struct CanBusRx {
    can_rx: CanRx<'static>,
}

impl CanBusRXTrait for CanBusRx {
    type Error = BusError;
    type Message = EnvelopeWrapper;

    async fn receive(&mut self) -> Result<Self::Message, Self::Error> {
        let envelope = self.can_rx.read().await?;
        Ok(EnvelopeWrapper(envelope))
    }
}
