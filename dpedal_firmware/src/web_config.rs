use defmt::*;
use dpedal_config::web_config_protocol::{Request, Response};
use embassy_rp::usb::{Endpoint, In, Out};
use embassy_rp::{peripherals::USB, usb::Driver};
//use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};
use embassy_usb::Builder;
use embassy_usb::class::web_usb::{Config as WebUsbConfig, State, WebUsb};
use embassy_usb::driver::{Endpoint as EndpointTrait, EndpointIn, EndpointOut};
use embassy_usb::msos::{self, windows_version};
use embassy_usb::types::InterfaceNumber;
use postcard::accumulator::CobsAccumulator;
use static_cell::StaticCell;

use crate::config::ConfigFlash;

// This is a randomly generated GUID to allow clients on Windows to find our device
const DEVICE_INTERFACE_GUIDS: &[&str] = &["{da327103-02a8-4d8a-8329-be81cdb97cc7}"];

pub struct WebConfig {
    write_ep: Endpoint<'static, USB, In>,
    read_ep: Endpoint<'static, USB, Out>,
    config_flash: ConfigFlash,
}

//pub static CONFIG_CHANNEL: Channel<ThreadModeRawMutex, (), 64> = Channel::new();

impl WebConfig {
    pub fn new(
        builder: &mut Builder<'static, Driver<'static, USB>>,
        config_flash: ConfigFlash,
    ) -> Self {
        static WEBUSB_CONFIG: StaticCell<WebUsbConfig> = StaticCell::new();
        let webusb_config = WEBUSB_CONFIG.init(WebUsbConfig {
            max_packet_size: 64,
            vendor_code: 1,
            // This sounds useful but in reality is really annoying.
            // TODO: Maybe we can make it pop-up only in certain circumstances?
            //landing_url: Some(Url::new("https://dpedal.com/config.html")),
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

        Self {
            write_ep,
            read_ep,
            config_flash,
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
            let mut cobs_buf: CobsAccumulator<1024> = CobsAccumulator::new();
            let request = loop {
                let n = self.read_ep.read(&mut packet_buf).await.unwrap();
                match cobs_buf.feed::<Request>(&packet_buf[..n]) {
                    postcard::accumulator::FeedResult::Consumed => {}
                    postcard::accumulator::FeedResult::OverFull(_items) => {
                        error!("request exceeded 1024 bytes");
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
                Request::GetConfig => Response::GetConfig(
                    self.config_flash
                        .load_config_bytes_from_flash()
                        .map(|x| x.0),
                ),
                Request::SetConfig(array_vec) => {
                    defmt::info!("set config {:?}", array_vec.as_ref());
                    if let Err(()) = self
                        .config_flash
                        .load_config_bytes_to_flash_and_reload_config(array_vec)
                        .await
                    {
                        // TODO: return error over protocol
                        defmt::panic!("Config invalid, not writing to flash")
                    }
                    Response::SetConfig
                }
            };

            self.send_response(response).await;
        }
    }

    async fn send_response(&mut self, response: Response) {
        let mut response_buf = [0; 1024];
        let response = postcard::to_slice_cobs(&response, &mut response_buf).unwrap();
        info!("responded with message containing {} bytes", response.len());
        for chunk in response.chunks(64) {
            if !chunk.is_empty() {
                self.write_ep.write(chunk).await.unwrap();
            }
        }
    }
}
