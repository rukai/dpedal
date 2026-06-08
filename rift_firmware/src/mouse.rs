use defmt::*;
use embassy_futures::join::join;
use embassy_rp::{peripherals::USB, usb::Driver};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};
use embassy_usb::{
    Builder,
    class::hid::{HidBootProtocol, HidReader, HidReaderWriter, HidSubclass, HidWriter, State},
};
use rift_config::MouseInput;
use static_cell::StaticCell;
use usbd_hid::descriptor::{MouseReport, SerializedDescriptor};

use crate::usb::MyRequestHandler;

pub struct Mouse {
    reader: Option<HidReader<'static, Driver<'static, USB>, 1>>,
    writer: HidWriter<'static, Driver<'static, USB>, 8>,
}

pub static MOUSE_CHANNEL: Channel<ThreadModeRawMutex, MouseEvent, 64> = Channel::new();

impl Mouse {
    pub fn new(builder: &mut Builder<'static, Driver<'static, USB>>) -> Self {
        let config = embassy_usb::class::hid::Config {
            hid_subclass: HidSubclass::Boot,
            hid_boot_protocol: HidBootProtocol::Mouse,
            report_descriptor: MouseReport::desc(),
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
        let mut report = MouseReport {
            buttons: 0,
            x: 0,
            y: 0,
            wheel: 0,
            pan: 0,
        };
        let mut cursor_x = Axis::new();
        let mut cursor_y = Axis::new();
        let mut scroll_x = Axis::new();
        let mut scroll_y = Axis::new();

        loop {
            // Delay processing events until we are able to actually send the report to ensure the report contains the most up to date information.
            // TODO: Actually check behaviour of this await and write_serialize await, do they actually block until host has polled us?
            self.writer.ready().await;

            while let Ok(event) = MOUSE_CHANNEL.try_receive() {
                match event {
                    MouseEvent::Pressed(input) => match input {
                        MouseInput::ScrollUp(value) => scroll_y.add_velocity(value as i32),
                        MouseInput::ScrollDown(value) => scroll_y.sub_velocity(value as i32),
                        MouseInput::ScrollLeft(value) => scroll_x.sub_velocity(value as i32),
                        MouseInput::ScrollRight(value) => scroll_x.add_velocity(value as i32),
                        MouseInput::MoveUp(value) => cursor_y.sub_velocity(value as i32),
                        MouseInput::MoveDown(value) => cursor_y.add_velocity(value as i32),
                        MouseInput::MoveLeft(value) => cursor_x.sub_velocity(value as i32),
                        MouseInput::MoveRight(value) => cursor_x.add_velocity(value as i32),
                        MouseInput::ClickLeft => report.buttons |= 0b0000_0001,
                        MouseInput::ClickRight => report.buttons |= 0b0000_0010,
                        MouseInput::ClickMiddle => report.buttons |= 0b0000_0100,
                    },
                    MouseEvent::Released(input) => match input {
                        MouseInput::ScrollUp(value) => scroll_y.sub_velocity(value as i32),
                        MouseInput::ScrollDown(value) => scroll_y.add_velocity(value as i32),
                        MouseInput::ScrollLeft(value) => scroll_x.add_velocity(value as i32),
                        MouseInput::ScrollRight(value) => scroll_x.sub_velocity(value as i32),
                        MouseInput::MoveUp(value) => cursor_y.add_velocity(value as i32),
                        MouseInput::MoveDown(value) => cursor_y.sub_velocity(value as i32),
                        MouseInput::MoveLeft(value) => cursor_x.add_velocity(value as i32),
                        MouseInput::MoveRight(value) => cursor_x.sub_velocity(value as i32),
                        MouseInput::ClickLeft => report.buttons &= 0b1111_1110,
                        MouseInput::ClickRight => report.buttons &= 0b1111_1101,
                        MouseInput::ClickMiddle => report.buttons &= 0b1111_1011,
                    },
                }
            }

            report.x = cursor_x.tick();
            report.y = cursor_y.tick();
            report.pan = scroll_x.tick();
            report.wheel = scroll_y.tick();

            match self.writer.write_serialize(&report).await {
                Ok(()) => {}
                Err(e) => warn!("Failed to send report: {:?}", e),
            };
        }
    }
}

#[allow(unused)]
#[derive(Clone, Copy)]
pub enum MouseEvent {
    Pressed(MouseInput),
    Released(MouseInput),
}

// poll_ms is 1, so there are 1000 ticks per second
const TICKS_PER_SECOND: i32 = 1000;

struct Axis {
    velocity: i32,  // sum of active velocities (pixels/sec)
    remainder: i32, // sub-pixel accumulator in 1/TICKS_PER_SECOND pixel units
}

impl Axis {
    fn new() -> Self {
        Axis {
            velocity: 0,
            remainder: 0,
        }
    }

    fn add_velocity(&mut self, v: i32) {
        self.velocity += v;
    }

    fn sub_velocity(&mut self, v: i32) {
        self.velocity -= v;
    }

    fn tick(&mut self) -> i8 {
        self.remainder += self.velocity;
        let pixels = self.remainder / TICKS_PER_SECOND;
        self.remainder %= TICKS_PER_SECOND;
        pixels.clamp(-127, 127) as i8
    }
}
