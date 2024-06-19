use core::cell::RefCell;

use embassy_stm32::{
    bind_interrupts,
    can::{
        self,
        frame::{FdEnvelope, FdFrame},
        Can, CanConfigurator,
    },
    gpio::{Input, Level, Output, Pull, Speed},
    peripherals::{FDCAN1, PD0, PD1, PD2, PD3, PD5},
};
use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    pubsub::{PubSubChannel, Publisher, Subscriber},
};
use embedded_can::{ExtendedId, Id};
use firmware_common::driver::can_bus::{
    CanBusMessage as CanBusMessageTrait, CanBusRX as CanBusRXTrait, CanBusTX as CanBusTXTrait,
    CanBusTXFrame, SplitableCanBus,
};
use futures::join;
use heapless::Vec;

use crate::sleep;

bind_interrupts!(struct Irqs {
    FDCAN1_IT0 => can::IT0InterruptHandler<FDCAN1>;
    FDCAN1_IT1 => can::IT1InterruptHandler<FDCAN1>;
});

struct CanBusRunner {
    en: Output<'static>,
    stb_n: Output<'static>,
    err_n: Input<'static>,
    fdcan: Can<'static>,
}

impl CanBusRunner {
    async fn run(mut self, can_bus: &CanBus) {
        // reset
        self.en.set_low();
        self.stb_n.set_low();
        sleep!(10);
        self.en.set_high();
        self.stb_n.set_high();
        sleep!(10);

        let mut tx_frame_subscriber = can_bus.tx_channel.subscriber().unwrap();
        let rx_envelope_publisher = can_bus.rx_channel.publisher().unwrap();
        let (mut tx, mut rx, _) = self.fdcan.split();
        let tx_fut = async {
            loop {
                let frame = tx_frame_subscriber.next_message_pure().await;
                if let Some(returned_frame) = tx.write_fd(&frame).await {
                    tx.write_fd(&returned_frame).await;
                }
            }
        };
        let rx_fut = async {
            loop {
                match rx.read_fd().await {
                    Ok(envelope) => {
                        rx_envelope_publisher.publish_immediate(envelope.into());
                    }
                    Err(e) => {
                        defmt::warn!("CAN bus error: {:?}", e);
                        sleep!(500);
                    }
                }
            }
        };

        join!(tx_fut, rx_fut);
    }
}

#[derive(Debug, Clone, defmt::Format)]
pub struct CanBusMessage {
    pub id: u32,
    pub rtr: Option<usize>,
    pub data: Vec<u8, 64>,
}

impl CanBusMessageTrait for CanBusMessage {
    fn id(&self) -> u32 {
        self.id
    }

    fn rtr(&self) -> Option<usize> {
        self.rtr
    }

    fn data(&self) -> &[u8] {
        &self.data
    }
}

pub struct CanBus {
    runner: RefCell<Option<CanBusRunner>>,
    tx_channel: PubSubChannel<NoopRawMutex, FdFrame, 2, 1, 3>,
    rx_channel: PubSubChannel<NoopRawMutex, CanBusMessage, 2, 3, 1>,
}

impl CanBus {
    pub fn new(fdcan: FDCAN1, rx: PD0, tx: PD1, en: PD3, stb_n: PD5, err_n: PD2) -> Self {
        let mut can_config: CanConfigurator<_> = CanConfigurator::new(fdcan, rx, tx, Irqs);
        can_config.set_bitrate(250_000);
        can_config.set_fd_data_bitrate(250_000, false);
        let can = can_config.into_normal_mode();

        Self {
            runner: RefCell::new(Some(CanBusRunner {
                en: Output::new(en, Level::Low, Speed::Low),
                stb_n: Output::new(stb_n, Level::Low, Speed::Low),
                err_n: Input::new(err_n, Pull::None),
                fdcan: can,
            })),
            tx_channel: PubSubChannel::new(),
            rx_channel: PubSubChannel::new(),
        }
    }

    pub async fn run(&self) {
        let mut runner = self.runner.borrow_mut();
        runner.take().unwrap().run(self).await;
    }
}

impl SplitableCanBus for &CanBus {
    type Error = ();

    async fn reset(&mut self) -> Result<(), Self::Error> {
        // noop
        Ok(())
    }

    fn split(
        &mut self,
    ) -> (
        impl CanBusTXTrait<Error = Self::Error>,
        impl CanBusRXTrait<Error = Self::Error>,
    ) {
        (CanBusTx::new(self), CanBusRx::new(self))
    }
}

impl From<FdEnvelope> for CanBusMessage {
    fn from(envelope: FdEnvelope) -> Self {
        let header = envelope.frame.header();
        Self {
            id: match header.id() {
                Id::Standard(id) => id.as_raw() as u32,
                Id::Extended(id) => id.as_raw(),
            },
            rtr: if header.rtr() {
                Some(header.len() as usize)
            } else {
                None
            },
            data: Vec::from_slice(envelope.frame.data()).unwrap(),
        }
    }
}

struct CanBusTx<'a> {
    can_bus: &'a CanBus,
    publisher: Publisher<'a, NoopRawMutex, FdFrame, 2, 1, 3>,
}

impl<'a> CanBusTx<'a> {
    fn new(can_bus: &'a CanBus) -> Self {
        Self {
            can_bus,
            publisher: can_bus.tx_channel.publisher().unwrap(),
        }
    }
}

impl CanBusTXTrait for CanBusTx<'_> {
    type Error = ();

    async fn send(&mut self, frame: CanBusTXFrame) -> Result<(), Self::Error> {
        match frame {
            CanBusTXFrame::Data { id, data } => {
                self.publisher
                    .publish(FdFrame::new_extended(id, data.as_slice()).unwrap())
                    .await;
                Ok(())
            }
            CanBusTXFrame::Remote { id, length } => {
                self.publisher
                    .publish(
                        FdFrame::new_remote(Id::Extended(ExtendedId::new(id).unwrap()), length)
                            .unwrap(),
                    )
                    .await;
                Ok(())
            }
        }
    }
}

struct CanBusRx<'a> {
    can_bus: &'a CanBus,
    subscriber: Subscriber<'a, NoopRawMutex, CanBusMessage, 2, 3, 1>,
}

impl<'a> CanBusRx<'a> {
    fn new(can_bus: &'a CanBus) -> Self {
        Self {
            can_bus,
            subscriber: can_bus.rx_channel.subscriber().unwrap(),
        }
    }
}

impl<'a> CanBusRXTrait for CanBusRx<'a> {
    type Error = ();

    async fn receive(&mut self) -> Result<impl CanBusMessageTrait, Self::Error> {
        Ok(self.subscriber.next_message_pure().await)
    }
}
