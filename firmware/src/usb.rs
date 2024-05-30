use core::mem::MaybeUninit;
use embassy_stm32::bind_interrupts;
use embassy_stm32::peripherals::{PA11, PA12, USB_OTG_HS};
use embassy_stm32::usb::{self, Config as USBDriverConfig, Driver};
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use embassy_usb::Config as USBConfig;
use embassy_usb::{Builder, UsbDevice};
use firmware_common::driver::usb::USB as CommonUSB;
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

pub struct UsbRunner {
    usb_device: UsbDevice<'static, Driver<'static, USB_OTG_HS>>,
}

impl UsbRunner {
    pub async fn run(&mut self) {
        self.usb_device.run().await;
    }
}

pub struct Usb {
    class: CdcAcmClass<'static, Driver<'static, USB_OTG_HS>>,
}

impl Usb {
    pub fn new(usb: USB_OTG_HS, dp: PA12, dm: PA11) -> (Self, UsbRunner) {
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

        (Self { class }, UsbRunner { usb_device })
    }
}

#[derive(defmt::Format)]
pub enum UsbError {
    Timeout,
    EndPoint(EndpointError),
}

impl CommonUSB for Usb {
    type Error = UsbError;

    async fn write_64b(&mut self, data: &[u8]) -> Result<(), UsbError> {
        let write_fut = self.class.write_packet(data);
        let timeout_fut = Timer::after(Duration::from_millis(1000));
        pin_mut!(write_fut);
        pin_mut!(timeout_fut);
        match select(write_fut, timeout_fut).await {
            Either::Left((Ok(_), _)) => Ok(()),
            Either::Left((Err(e), _)) => Err(UsbError::EndPoint(e)),
            Either::Right(_) => Err(UsbError::Timeout),
        }
    }

    async fn read(&mut self, buffer: &mut [u8]) -> Result<usize, UsbError> {
        let length = self.class.read_packet(buffer).await;
        match length {
            Ok(length) => Ok(length),
            Err(e) => Err(UsbError::EndPoint(e)),
        }
    }

    async fn wait_connection(&mut self) {
        self.class.wait_connection().await
    }
}
