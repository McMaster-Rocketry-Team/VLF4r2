use core::mem;

use defmt::info;
use embassy_stm32::{
    bind_interrupts,
    can::{self, enums::BusError, frame::Envelope, CanConfigurator, CanRx, CanTx, Frame},
    gpio::{Level, Output, Speed},
    peripherals::{FDCAN1, PD0, PD1, PD3, PD5},
};
use embedded_can::{ExtendedId, Id};
use firmware_common::{
    common::can_bus::message::CanBusMessage,
    driver::can_bus::{
        CanBusRX as CanBusRXTrait, CanBusRawMessage, CanBusTX as CanBusTXTrait,
        SplitableCanBusWrapper,
    },
};

use crate::sleep;

bind_interrupts!(struct Irqs {
    FDCAN1_IT0 => can::IT0InterruptHandler<FDCAN1>;
    FDCAN1_IT1 => can::IT1InterruptHandler<FDCAN1>;
});

pub struct CanBus;

impl CanBus {
    pub async fn new(
        fdcan: FDCAN1,
        rx: PD0,
        tx: PD1,
        en: PD3,
        stb_n: PD5,
    ) -> SplitableCanBusWrapper<CanBusTx, CanBusRx, BusError> {
        let mut en = Output::new(en, Level::Low, Speed::Low);
        let mut stb_n = Output::new(stb_n, Level::Low, Speed::Low);
        en.set_low();
        stb_n.set_low();
        sleep!(10);
        en.set_high();
        stb_n.set_high();
        sleep!(10);
        mem::forget(en);
        mem::forget(stb_n);

        let mut can_config = CanConfigurator::new(fdcan, rx, tx, Irqs);
        can_config.set_bitrate(250_000);
        // can_config.set_config(mode)
        let can = can_config.into_normal_mode();

        let (tx, rx, _) = can.split();
        let tx = CanBusTx {
            can_tx: tx,
            self_node: (0, 0),
        };
        let rx = CanBusRx { can_rx: rx };

        SplitableCanBusWrapper::new(tx, rx)
    }
}

pub struct CanBusTx {
    can_tx: CanTx<'static>,
    // node type, node id
    self_node: (u8, u16),
}

impl CanBusTXTrait for CanBusTx {
    type Error = BusError;

    async fn send<T: CanBusMessage>(
        &mut self,
        message: &T,
        priority: u8,
    ) -> Result<(), Self::Error> {
        let frame = Frame::new_extended(
            T::create_id(priority, self.self_node.0, self.self_node.1).into(),
            &message.to_data(),
        )
        .unwrap();
        info!("Sending CAN frame");
        if let Some(returned_frame) = self.can_tx.write(&frame).await {
            info!("Sending returned CAN frame");
            self.can_tx.write(&returned_frame).await;
        }
        info!("Sent CAN frame");
        Ok(())
    }

    async fn send_remote<T: CanBusMessage>(&mut self, priority: u8) -> Result<(), Self::Error> {
        let frame = Frame::new_remote(
            Id::Extended(
                ExtendedId::new(T::create_id(priority, self.self_node.0, self.self_node.1).into())
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

    fn configure_self_node(&mut self, node_type: u8, node_id: u16) {
        self.self_node = (node_type, node_id);
    }
}

pub struct EnvelopeWrapper(Envelope);

impl CanBusRawMessage for EnvelopeWrapper {
    fn timestamp(&self) -> f64 {
        self.0.ts.as_micros() as f64 / 1000.0
    }

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
