use core::sync::atomic::{AtomicBool, Ordering};
use defmt::*;
use dpedal_config::MAX_NICKNAME_LEN;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_rp::{Peri, bind_interrupts};
use embassy_usb::class::hid::{ReportId, RequestHandler};
use embassy_usb::control::OutResponse;
use embassy_usb::{Builder, Config, Handler};
use heapless::String;
use static_cell::StaticCell;

use crate::config::CONFIG;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

pub async fn usb_builder(usb: Peri<'static, USB>) -> Builder<'static, Driver<'static, USB>> {
    let driver = Driver::new(usb, Irqs);

    // Create embassy-usb DeviceBuilder using the driver and config.
    static DEVICE_HANDLER: StaticCell<MyDeviceHandler> = StaticCell::new();

    static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static MSOS_DESC: StaticCell<[u8; 1024]> = StaticCell::new();
    static CONTROL_BUF: StaticCell<[u8; 128]> = StaticCell::new();
    static PRODUCT_NAME: StaticCell<String<{ MAX_NICKNAME_LEN + 20 }>> = StaticCell::new();

    let mut config = Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some("Rukai");
    let product = PRODUCT_NAME.init(String::try_from("DPedal").unwrap());
    let nickname = CONFIG.lock().await.as_ref().unwrap().nickname.clone();
    if !nickname.is_empty() {
        product.push_str(" - ").unwrap();
        product.push_str(&nickname).unwrap();
    }
    match env!("PROFILE") {
        "release" => {}
        _ => product.push_str(" (debug build)").unwrap(),
    }
    config.product = Some(product);
    config.serial_number = None;
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    let mut builder = Builder::new(
        driver,
        config,
        &mut CONFIG_DESC.init([0; 256])[..],
        &mut BOS_DESC.init([0; 256])[..],
        &mut MSOS_DESC.init([0; 1024])[..],
        &mut CONTROL_BUF.init([0; 128])[..],
    );

    builder.handler(DEVICE_HANDLER.init(MyDeviceHandler::new()));
    builder
}

pub struct MyRequestHandler {}

impl RequestHandler for MyRequestHandler {
    fn get_report(&mut self, id: ReportId, _buf: &mut [u8]) -> Option<usize> {
        info!("Get report for {:?}", id);
        None
    }

    fn set_report(&mut self, id: ReportId, data: &[u8]) -> OutResponse {
        info!("Set report for {:?}: {=[u8]}", id, data);
        OutResponse::Accepted
    }

    fn set_idle_ms(&mut self, id: Option<ReportId>, dur: u32) {
        info!("Set idle rate for {:?} to {:?}", id, dur);
    }

    fn get_idle_ms(&mut self, id: Option<ReportId>) -> Option<u32> {
        info!("Get idle rate for {:?}", id);
        None
    }
}

struct MyDeviceHandler {
    configured: AtomicBool,
}

impl MyDeviceHandler {
    fn new() -> Self {
        MyDeviceHandler {
            configured: AtomicBool::new(false),
        }
    }
}

impl Handler for MyDeviceHandler {
    fn enabled(&mut self, enabled: bool) {
        self.configured.store(false, Ordering::Relaxed);
        if enabled {
            info!("Device enabled");
        } else {
            info!("Device disabled");
        }
    }

    fn reset(&mut self) {
        self.configured.store(false, Ordering::Relaxed);
        info!("Bus reset, the Vbus current limit is 100mA");
    }

    fn addressed(&mut self, addr: u8) {
        self.configured.store(false, Ordering::Relaxed);
        info!("USB address set to: {}", addr);
    }

    fn configured(&mut self, configured: bool) {
        self.configured.store(configured, Ordering::Relaxed);
        if configured {
            info!(
                "Device configured, it may now draw up to the configured current limit from Vbus."
            )
        } else {
            info!("Device is no longer configured, the Vbus current limit is 100mA.");
        }
    }
}
