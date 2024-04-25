use embassy_stm32::gpio::AnyPin;
use embassy_stm32::spi::{Error as EmbassySpiError, Instance, RxDma, TxDma, Word};
use embassy_stm32::{gpio::Output, spi::Spi};
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::mutex::Mutex;

use crate::utils::run_with_timeout;

#[derive(defmt::Format, Debug)]
pub enum SpiBusError {
    Timeout { ms: u64 },
    Bus(EmbassySpiError),
}

pub trait SharedSpiBus {
    async fn transfer<'a, W: Word>(
        &self,
        read: &mut [W],
        write: &[W],
        cs: &mut Output<'a, AnyPin>,
    ) -> Result<(), SpiBusError>;

    async fn transfer_in_place<'a, W: Word>(
        &self,
        data: &mut [W],
        cs: &mut Output<'a, AnyPin>,
    ) -> Result<(), SpiBusError>;
}

impl<'d, M: RawMutex, T: Instance, Tx: TxDma<T>, Rx: RxDma<T>> SharedSpiBus
    for Mutex<M, Spi<'d, T, Tx, Rx>>
{
    async fn transfer<'a, W: Word>(
        &self,
        read: &mut [W],
        write: &[W],
        cs: &mut Output<'a, AnyPin>,
    ) -> Result<(), SpiBusError> {
        let mut spi = self.lock().await;
        cs.set_low();

        let result = match run_with_timeout(100, spi.transfer(read, write)).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(SpiBusError::Bus(e)),
            Err(ms) => Err(SpiBusError::Timeout { ms }),
        };

        cs.set_high();
        result
    }

    async fn transfer_in_place<'a, W: Word>(
        &self,
        data: &mut [W],
        cs: &mut Output<'a, AnyPin>,
    ) -> Result<(), SpiBusError> {
        let mut spi = self.lock().await;
        cs.set_low();

        let result = match run_with_timeout(100, spi.transfer_in_place(data)).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(SpiBusError::Bus(e)),
            Err(ms) => Err(SpiBusError::Timeout { ms }),
        };

        cs.set_high();
        result
    }
}