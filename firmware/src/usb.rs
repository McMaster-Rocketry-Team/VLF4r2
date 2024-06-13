use core::cell::RefCell;
use core::mem::MaybeUninit;
use defmt::debug;
use embassy_stm32::bind_interrupts;
use embassy_stm32::peripherals::{PA11, PA12, USB_OTG_HS};
use embassy_stm32::usb::{self, Config as USBDriverConfig, Driver};
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_acm::{CdcAcmClass, Receiver, Sender, State};
use embassy_usb::driver::EndpointError;
use embassy_usb::Config as USBConfig;
use embassy_usb::{Builder, UsbDevice};
use firmware_common::driver::serial::SplitableSerial;
use firmware_common::driver::usb::SplitableUSB;
use futures::future::{select, Either};
use futures::pin_mut;
use stm32_device_signature::device_id_hex;

bind_interrupts!(struct Irqs {
    OTG_HS => usb::InterruptHandler<USB_OTG_HS>;
});

static mut EP_OUT_BUFFER: [u8; 256] = [0u8; 256];
static mut DEVICE_DESCRIPTOR: [u8; 256] = [0; 256];
static mut CONFIG_DESCRIPTOR: [u8; 256] = [0; 256];
static mut BOS_DESCRIPTOR: [u8; 256] = [0; 256];
static mut CONTROL_BUF: [u8; 64] = [0; 64];
static mut STATE: MaybeUninit<State> = MaybeUninit::uninit();

pub struct UsbRunner<'a> {
    usb: &'a Usb,
}

impl<'a> UsbRunner<'a> {
    pub async fn run(&mut self) {
        let mut usb_device = self.usb.usb_device.borrow_mut();
        usb_device.run().await;
    }
}

pub struct Usb {
    usb_device: RefCell<UsbDevice<'static, Driver<'static, USB_OTG_HS>>>,
    tx: RefCell<Sender<'static, Driver<'static, USB_OTG_HS>>>,
    rx: RefCell<Receiver<'static, Driver<'static, USB_OTG_HS>>>,
}

impl Usb {
    pub fn new(usb: USB_OTG_HS, dp: PA12, dm: PA11) -> Self {
        let mut config = USBDriverConfig::default();
        config.vbus_detection = true;
        let driver = Driver::new_fs(usb, Irqs, dp, dm, unsafe { &mut EP_OUT_BUFFER }, config);

        let mut config = USBConfig::new(0xc0de, 0xcafe);

        config.self_powered = true;
        config.manufacturer = Some("MacRocketry");
        config.product = Some("Void Lake Fusion 4 r2");
        config.serial_number = Some(device_id_hex());

        // Required for windows compatibility.
        config.device_class = 0xEF;
        config.device_sub_class = 0x02;
        config.device_protocol = 0x01;
        config.composite_with_iads = true;

        unsafe {
            STATE.write(State::new());
        }
        let mut builder = Builder::new(
            driver,
            config,
            unsafe { &mut DEVICE_DESCRIPTOR },
            unsafe { &mut CONFIG_DESCRIPTOR },
            unsafe { &mut BOS_DESCRIPTOR },
            unsafe { &mut CONTROL_BUF },
        );
        let class = CdcAcmClass::new(&mut builder, unsafe { STATE.write(State::new()) }, 64);
        let usb_device = builder.build();
        let (tx, rx) = class.split();
        Self {
            usb_device: RefCell::new(usb_device),
            tx: RefCell::new(tx),
            rx: RefCell::new(rx),
        }
    }

    pub fn get_runner(&self) -> UsbRunner {
        UsbRunner { usb: self }
    }
}

#[derive(defmt::Format, Debug)]
pub enum UsbError {
    Timeout,
    EndPoint(EndpointError),
}

impl embedded_io_async::Error for UsbError {
    fn kind(&self) -> embedded_io_async::ErrorKind {
        match self {
            UsbError::Timeout => embedded_io_async::ErrorKind::TimedOut,
            UsbError::EndPoint(EndpointError::BufferOverflow) => {
                embedded_io_async::ErrorKind::OutOfMemory
            }
            UsbError::EndPoint(EndpointError::Disabled) => embedded_io_async::ErrorKind::Other,
        }
    }
}

impl SplitableUSB for &Usb {
    async fn wait_connection(&mut self) {
        let mut rx = self.rx.borrow_mut();
        rx.wait_connection().await;
    }
}

impl SplitableSerial for &Usb {
    type Error = UsbError;

    fn split(
        &mut self,
    ) -> (
        impl embedded_io_async::Write<Error = Self::Error>,
        impl embedded_io_async::Read<Error = Self::Error>,
    ) {
        let tx = TXGuard { usb: self };
        let rx = RXGuard {
            usb: self,
            buffer: [0u8; 64],
            buffer_len: 0,
        };
        (tx, rx)
    }
}

struct TXGuard<'a> {
    usb: &'a Usb,
}

impl<'a> embedded_io_async::ErrorType for TXGuard<'a> {
    type Error = UsbError;
}

impl<'a> embedded_io_async::Write for TXGuard<'a> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        // can only write 64 bytes at a time
        debug!("Writing to USB, requested len: {}", buf.len());
        let mut tx = self.usb.tx.borrow_mut();

        let write_fut = async {
            let result: Result<usize, EndpointError> = try {
                if buf.len() > 64 {
                    debug!("Buffer is larger than 64 bytes");
                    tx.write_packet(&buf[..64]).await?;
                    64
                } else if buf.len() == 64 {
                    debug!("Buffer is 64 bytes");
                    tx.write_packet(buf).await?;
                    tx.write_packet(&[]).await?;
                    64
                } else {
                    debug!("Buffer is smaller than 64 bytes");
                    tx.write_packet(buf).await?;
                    buf.len()
                }
            };
            result
        };

        let timeout_fut = Timer::after(Duration::from_millis(1000));
        pin_mut!(write_fut);
        pin_mut!(timeout_fut);
        match select(write_fut, timeout_fut).await {
            Either::Left((Ok(len), _)) => Ok(len),
            Either::Left((Err(e), _)) => Err(UsbError::EndPoint(e)),
            Either::Right(_) => Err(UsbError::Timeout),
        }
    }
}

struct RXGuard<'a> {
    usb: &'a Usb,
    buffer: [u8; 64],
    buffer_len: usize,
}

impl<'a> embedded_io_async::ErrorType for RXGuard<'a> {
    type Error = UsbError;
}

impl<'a> embedded_io_async::Read for RXGuard<'a> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        debug!("Reading from USB, requested len: {}", buf.len());
        if self.buffer_len > 0 {
            debug!("Using buffer");
            if self.buffer_len > buf.len() {
                debug!("Buffer is larger than requested");
                buf.copy_from_slice(&self.buffer[..buf.len()]);
                self.buffer.copy_within(buf.len()..self.buffer_len, 0);
                self.buffer_len -= buf.len();
                Ok(buf.len())
            } else {
                debug!("Buffer is smaller than requested");
                buf[..self.buffer_len].copy_from_slice(&self.buffer[..self.buffer_len]);
                let len = self.buffer_len;
                self.buffer_len = 0;
                Ok(len)
            }
        } else {
            debug!("Reading from USB, no buffer");
            let mut rx = self.usb.rx.borrow_mut();
            let length = rx.read_packet(&mut self.buffer).await;
            match length {
                Ok(length) => {
                    if length > buf.len() {
                        debug!("Read length is larger than requested");
                        buf.copy_from_slice(&self.buffer[..buf.len()]);
                        self.buffer.copy_within(buf.len()..length, 0);
                        self.buffer_len = length - buf.len();
                        Ok(buf.len())
                    } else {
                        debug!("Read length is smaller or equal than requested");
                        buf[..length].copy_from_slice(&self.buffer[..length]);
                        Ok(length)
                    }
                }
                Err(e) => Err(UsbError::EndPoint(e)),
            }
        }
    }
}
