use defmt::*;
use dpedal_config::COBS_ACCUMULATOR_SIZE;
use dpedal_config::web_config_protocol::{Request, Response};
use embassy_rp::usb::{Endpoint, In, Out};
use embassy_rp::{peripherals::USB, usb::Driver};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_usb::Builder;
use embassy_usb::class::web_usb::{Config as WebUsbConfig, State, WebUsb};
use embassy_usb::driver::{Endpoint as EndpointTrait, EndpointIn, EndpointOut};
use embassy_usb::msos::{self, windows_version};
use embassy_usb::types::InterfaceNumber;
use postcard::accumulator::CobsAccumulator;
use static_cell::StaticCell;

use crate::config::{CONFIG_UPDATED, ConfigFlash};

// This is a randomly generated GUID to allow clients on Windows to find our device
const DEVICE_INTERFACE_GUIDS: &[&str] = &["{da327103-02a8-4d8a-8329-be81cdb97cc7}"];

pub struct WebConfig {
    write_ep: Endpoint<'static, USB, In>,
    read_ep: Endpoint<'static, USB, Out>,
    config_flash: &'static Mutex<CriticalSectionRawMutex, ConfigFlash>,
    cobs_buf: &'static mut CobsAccumulator<COBS_ACCUMULATOR_SIZE>,
    response_buf: &'static mut [u8; COBS_ACCUMULATOR_SIZE],
}

impl WebConfig {
    pub fn new(
        builder: &mut Builder<'static, Driver<'static, USB>>,
        config_flash: &'static Mutex<CriticalSectionRawMutex, ConfigFlash>,
    ) -> Self {
        static WEBUSB_CONFIG: StaticCell<WebUsbConfig> = StaticCell::new();
        let webusb_config = WEBUSB_CONFIG.init(WebUsbConfig {
            max_packet_size: 64,
            vendor_code: 1,
            // Intentionally set the landing_url to None.
            // This feature sounds useful but in reality is really annoying for regular users.
            landing_url: None,
        });

        // Add the Microsoft OS Descriptor (MSOS/MOD) descriptor.
        // We tell Windows that this entire device is compatible with the "WINUSB" feature,
        // which causes it to use the built-in WinUSB driver automatically, which in turn
        // can be used by libusb/rusb software without needing a custom driver or INF file.
        builder.msos_descriptor(windows_version::WIN8_1, 0);
        builder.msos_writer().configuration(0);
        builder.msos_writer().function(InterfaceNumber(0));
        builder.msos_feature(msos::CompatibleIdFeatureDescriptor::new("WINUSB", ""));
        builder.msos_feature(msos::RegistryPropertyFeatureDescriptor::new(
            "DeviceInterfaceGUIDs",
            msos::PropertyData::RegMultiSz(DEVICE_INTERFACE_GUIDS),
        ));
        builder.msos_writer().function(InterfaceNumber(1));
        builder.msos_feature(msos::CompatibleIdFeatureDescriptor::new("WINUSB", ""));
        builder.msos_feature(msos::RegistryPropertyFeatureDescriptor::new(
            "DeviceInterfaceGUIDs",
            msos::PropertyData::RegMultiSz(DEVICE_INTERFACE_GUIDS),
        ));

        static STATE: StaticCell<State> = StaticCell::new();
        WebUsb::configure(builder, STATE.init(State::new()), webusb_config);

        let mut func = builder.function(0xff, 0x00, 0x00);
        let mut iface = func.interface();
        let mut alt = iface.alt_setting(0xff, 0x00, 0x00, None);

        let write_ep = alt.endpoint_bulk_in(None, 64);
        let read_ep = alt.endpoint_bulk_out(None, 64);

        static COBS_BUF: StaticCell<CobsAccumulator<COBS_ACCUMULATOR_SIZE>> = StaticCell::new();
        let cobs_buf = COBS_BUF.init(CobsAccumulator::new());

        static RESPONSE_BUF: StaticCell<[u8; COBS_ACCUMULATOR_SIZE]> = StaticCell::new();
        let response_buf = RESPONSE_BUF.init([0u8; COBS_ACCUMULATOR_SIZE]);

        Self {
            write_ep,
            read_ep,
            config_flash,
            cobs_buf,
            response_buf,
        }
    }

    pub async fn process(&mut self) {
        self.wait_connected().await;
        info!("Connected to web configurator");
        self.echo().await;
    }

    // Wait until the device's endpoints are enabled.
    async fn wait_connected(&mut self) {
        self.read_ep.wait_enabled().await
    }

    // Echo data back to the host.
    async fn echo(&mut self) {
        let mut packet_buf = [0; 64];
        'skip_request: loop {
            *self.cobs_buf = CobsAccumulator::new();
            let request = loop {
                let n = self.read_ep.read(&mut packet_buf).await.unwrap();
                match self.cobs_buf.feed::<Request>(&packet_buf[..n]) {
                    postcard::accumulator::FeedResult::Consumed => {}
                    postcard::accumulator::FeedResult::OverFull(_items) => {
                        error!("request exceeded buffer");
                        self.send_response(Response::ProtocolError).await;
                        continue 'skip_request;
                    }
                    postcard::accumulator::FeedResult::DeserError(_items) => {
                        error!("Failed to deserialize request");
                        self.send_response(Response::ProtocolError).await;
                        continue 'skip_request;
                    }
                    postcard::accumulator::FeedResult::Success { data, .. } => break data,
                }
            };
            let response = match request {
                Request::GetMeta => {
                    Response::GetMeta(self.config_flash.lock().await.load_meta_bytes().await)
                }
                Request::GetProfileCount => {
                    let count = self.config_flash.lock().await.get_profile_count().await;
                    Response::GetProfileCount(count)
                }
                Request::GetProfile(index) => Response::GetProfile(
                    self.config_flash
                        .lock()
                        .await
                        .load_profile_bytes(index)
                        .await,
                ),
                Request::SetMeta(bytes) => {
                    defmt::info!("set meta");
                    let result = self
                        .config_flash
                        .lock()
                        .await
                        .store_meta(bytes.as_slice())
                        .await;
                    Response::SetMeta(result)
                }
                Request::AddProfile(bytes) => {
                    defmt::info!("add profile");
                    let result = self
                        .config_flash
                        .lock()
                        .await
                        .add_profile(bytes.as_slice())
                        .await;
                    Response::AddProfile(result)
                }
                Request::ReloadConfig => {
                    CONFIG_UPDATED.sender().send(());
                    Response::ReloadConfig
                }
            };

            self.send_response(response).await;
        }
    }

    async fn send_response(&mut self, response: Response) {
        let response =
            postcard::to_slice_cobs(&response, self.response_buf.as_mut_slice()).unwrap();
        info!("responded with message containing {} bytes", response.len());
        for chunk in response.chunks(64) {
            if !chunk.is_empty() {
                self.write_ep.write(chunk).await.unwrap();
            }
        }
    }
}
