use dpedal_config::COBS_ACCUMULATOR_SIZE;
use dpedal_config::web_config_protocol::{Request, Response};
use futures::lock::Mutex;
use postcard::accumulator::CobsAccumulator;
use std::{cell::RefCell, rc::Rc};
use webusb_web::{OpenUsbDevice, Usb, UsbDevice, UsbDeviceFilter};

pub type DeviceList = Rc<RefCell<Vec<OpenDevice>>>;

pub struct OpenDevice {
    pub usb_device: UsbDevice,
    pub device: Rc<Device>,
}

pub struct Device {
    usb: OpenUsbDevice,
    lock: Mutex<()>,
}

impl Device {
    pub async fn new(devices: &DeviceList) -> Result<Rc<Self>, String> {
        let usb = Usb::new().map_err(|e| e.msg().to_string())?;

        let mut filter = UsbDeviceFilter::new();
        filter.vendor_id = Some(0xc0de);
        filter.product_id = Some(0xcafe);
        let usb_device = usb
            .request_device([filter])
            .await
            .map_err(|e| e.msg().to_string())?;

        if let Some(device) = devices
            .borrow()
            .iter()
            .find(|o| o.usb_device == usb_device)
            .map(|o| Rc::clone(&o.device))
        {
            return Ok(device);
        }

        let open_usb = usb_device
            .open()
            .await
            .map_err(|e| format!("Failed to open device: {e}"))?;

        log::info!("usb config {:#?}", usb_device.configuration());

        open_usb
            .claim_interface(1)
            .await
            .map_err(|e| e.msg().to_string())?;

        let device = Rc::new(Device {
            usb: open_usb,
            lock: Mutex::new(()),
        });
        devices.borrow_mut().push(OpenDevice {
            usb_device,
            device: Rc::clone(&device),
        });
        Ok(device)
    }

    pub async fn send_request(&self, request: &Request) -> Result<Response, String> {
        // Need to hold the lock for the duration of request/response pair
        let _lock = self.lock.lock().await;

        let request_bytes = postcard::to_stdvec_cobs(request).unwrap(); // TODO: when can this fail?
        self.usb
            .transfer_out(1, &request_bytes)
            .await
            .map_err(|e| format!("Failed to send request to device: {e}"))?;

        let mut cobs_buf: CobsAccumulator<COBS_ACCUMULATOR_SIZE> = CobsAccumulator::new();
        loop {
            let out = self
                .usb
                .transfer_in(1, 64)
                .await
                .map_err(|e| format!("Failed to receive response from device: {e}"))?;
            match cobs_buf.feed::<Response>(&out) {
                postcard::accumulator::FeedResult::Consumed => {}
                postcard::accumulator::FeedResult::OverFull(_items) => {
                    return Err("Device sent response > 1024 bytes".into());
                }
                postcard::accumulator::FeedResult::DeserError(_items) => {
                    return Err("Device sent response that could not be parsed.".into());
                }
                postcard::accumulator::FeedResult::Success { data, .. } => return Ok(data),
            }
        }
    }
}
