use defmt::*;
use dpedal_config::KeyboardInput;
use embassy_futures::join::join;
use embassy_rp::{peripherals::USB, usb::Driver};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};
use embassy_usb::{
    Builder,
    class::hid::{HidBootProtocol, HidReader, HidReaderWriter, HidSubclass, HidWriter, State},
};
use static_cell::StaticCell;
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};

use crate::usb::MyRequestHandler;

pub struct Keyboard {
    reader: Option<HidReader<'static, Driver<'static, USB>, 1>>,
    writer: HidWriter<'static, Driver<'static, USB>, 8>,
}

pub static KEYBOARD_CHANNEL: Channel<ThreadModeRawMutex, KeyboardEvent, 64> = Channel::new();

impl Keyboard {
    pub fn new(builder: &mut Builder<'static, Driver<'static, USB>>) -> Self {
        let config = embassy_usb::class::hid::Config {
            hid_subclass: HidSubclass::Boot,
            hid_boot_protocol: HidBootProtocol::Keyboard,
            report_descriptor: KeyboardReport::desc(),
            request_handler: None,
            poll_ms: 1,
            max_packet_size: 64,
        };
        static STATE: StaticCell<State> = StaticCell::new();
        let hid =
            HidReaderWriter::<'static, _, 1, 8>::new(builder, STATE.init(State::new()), config);
        let (reader, writer) = hid.split();

        Self {
            reader: Some(reader),
            writer,
        }
    }

    pub async fn process(&mut self) {
        let reader = self.reader.take().unwrap();

        join(
            self.process_write(),
            reader.run(false, &mut MyRequestHandler {}),
        )
        .await;
    }

    pub async fn process_write(&mut self) {
        let mut report = KeyboardReport {
            keycodes: [0, 0, 0, 0, 0, 0],
            leds: 0,
            modifier: 0,
            reserved: 0,
        };
        loop {
            // Delay processing events until we are able to actually send the report to ensure the report contains the most up to date information.
            // TODO: Actually check behaviour of this await and write_serialize await, do they actually block until host has polled us?
            self.writer.ready().await;

            while let Ok(event) = KEYBOARD_CHANNEL.try_receive() {
                match event {
                    KeyboardEvent::Pressed(key) => {
                        set_key(&mut report, key as u8);
                    }
                    KeyboardEvent::Released(key) => {
                        clear_key(&mut report, key as u8);
                    }
                };
            }

            // Send the report.
            match self.writer.write_serialize(&report).await {
                Ok(()) => {}
                Err(e) => warn!("Failed to send report: {:?}", e),
            };
        }
    }
}

/// First keycode in the USB HID modifier range (LeftControl through RightWindows).
/// Modifier keys must be sent via the modifier byte, not the keycodes array.
const MODIFIER_KEYCODE_START: u8 = 0xE0;

fn set_key(report: &mut KeyboardReport, keycode: u8) {
    if keycode >= MODIFIER_KEYCODE_START {
        // modifier field has a bit for each modifier keycode in order
        report.modifier |= 1 << (keycode - MODIFIER_KEYCODE_START);
        return;
    }

    // if keycode already set, do nothing
    for check_keycode in &mut report.keycodes {
        if *check_keycode == keycode {
            return;
        }
    }

    // Set an empty slot to the keycode
    for check_keycode in &mut report.keycodes {
        if *check_keycode == 0 {
            *check_keycode = keycode;
            return;
        }
    }
}

fn clear_key(report: &mut KeyboardReport, keycode: u8) {
    if keycode >= MODIFIER_KEYCODE_START {
        report.modifier &= !(1 << (keycode - MODIFIER_KEYCODE_START));
        return;
    }

    for check_keycode in &mut report.keycodes {
        if *check_keycode == keycode {
            *check_keycode = 0;
        }
    }
}

pub enum KeyboardEvent {
    Pressed(KeyboardInput),
    Released(KeyboardInput),
}
