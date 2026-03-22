#![no_main]
#![no_std]

//! Stack usage.
//! The DPedal firmware does not use an allocator.
//! Maybe it should, but its interesting to try writing software without an allocator,
//! and assuming we are well under the memory limit, using static buffers should make OoM issues impossible.
//!
//! However, currently dpedal stores huge values on the stack, which is problematic because:
//! * The RP2040 only has 256KB of RAM
//! * embassy/async exacerbates stack usage issues
//!
//! memory.x is setup to assert that the linker has allocated at least 64KB of stack.
//! Currently, the firmware works fine, but long term we should move anything >= 1024 bytes off of the stack to avoid stack overflow issues.

mod config;
mod input;
mod keyboard;
mod mapping_state;
mod mouse;
mod usb;
mod web_config;

use crate::config::ConfigFlash;
use crate::input::Inputs;
use crate::keyboard::Keyboard;
use crate::mouse::Mouse;
use crate::web_config::WebConfig;
use embassy_executor::Spawner;

use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    let config_flash = ConfigFlash::new(p.FLASH, p.DMA_CH0).await;

    let mut builder = usb::usb_builder(p.USB, config_flash).await;

    let mut web_config = WebConfig::new(&mut builder, config_flash);

    let mut keyboard = Keyboard::new(&mut builder);
    let mut mouse = Mouse::new(&mut builder);

    let mut usb = builder.build();

    // Run the USB device.
    let usb_fut = usb.run();

    let mut inputs = Inputs::new(
        config_flash,
        [
            Some(p.PIN_0.into()),
            Some(p.PIN_1.into()),
            Some(p.PIN_2.into()),
            Some(p.PIN_3.into()),
            Some(p.PIN_4.into()),
            Some(p.PIN_5.into()),
            Some(p.PIN_6.into()),
            Some(p.PIN_7.into()),
            Some(p.PIN_8.into()),
            Some(p.PIN_9.into()),
            Some(p.PIN_10.into()),
            Some(p.PIN_11.into()),
            Some(p.PIN_12.into()),
            Some(p.PIN_13.into()),
            Some(p.PIN_14.into()),
            Some(p.PIN_15.into()),
            Some(p.PIN_16.into()),
            Some(p.PIN_17.into()),
            Some(p.PIN_18.into()),
            Some(p.PIN_19.into()),
            Some(p.PIN_20.into()),
            Some(p.PIN_21.into()),
            Some(p.PIN_22.into()),
            Some(p.PIN_23.into()),
            Some(p.PIN_24.into()),
            Some(p.PIN_25.into()),
            Some(p.PIN_26.into()),
            Some(p.PIN_27.into()),
            Some(p.PIN_28.into()),
            Some(p.PIN_29.into()),
        ],
    );

    embassy_futures::join::join5(
        usb_fut,
        inputs.process(),
        keyboard.process(),
        mouse.process(),
        web_config.process(),
    )
    .await;
}
