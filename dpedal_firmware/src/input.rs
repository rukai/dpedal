use crate::config::{CONFIG_UPDATED, ConfigFlash};
use crate::mapping_state::MappingState;
use core::ops::ControlFlow;
use dpedal_config::{DpedalInput, MAX_MAPPINGS, Profile};
use embassy_rp::gpio::{AnyPin, Input, Pin, Pull};
use embassy_rp::{Peri, PeripheralType};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::Timer;
use heapless::Vec;
use static_cell::StaticCell;

static CURRENT_PROFILE: StaticCell<Profile> = StaticCell::new();

pub struct Inputs {
    pins: [Option<Peri<'static, AnyPin>>; 30],
    config_flash: &'static Mutex<CriticalSectionRawMutex, ConfigFlash>,
}

impl Inputs {
    pub fn new(
        config_flash: &'static Mutex<CriticalSectionRawMutex, ConfigFlash>,
        pins: [Option<Peri<'static, AnyPin>>; 30],
    ) -> Self {
        Inputs { pins, config_flash }
    }

    pub async fn process(&mut self) {
        let mut rx = CONFIG_UPDATED.receiver().unwrap();

        let mut button_left_pin = 13;
        let mut button_right_pin = 27;
        let mut dpad_up_pin = 26;
        let mut dpad_down_pin = 16;
        let mut dpad_left_pin = 17;
        let mut dpad_right_pin = 22;

        // Load initial config
        let meta = self.config_flash.lock().await.load_meta().await;
        let current_profile = {
            // TODO: currently handles config with 0 profiles by loading the default profile which has mappings in it.
            //       this is unintuitive and should be changed.
            let profile = self.config_flash.lock().await.load_profile(0).await;
            CURRENT_PROFILE.init(profile)
        };

        // Apply pin remappings from meta (set once; remappings not editable via web configurator)
        for remapping in &meta.pin_remappings {
            match remapping.input {
                DpedalInput::DpadUp => dpad_up_pin = remapping.pin as usize,
                DpedalInput::DpadDown => dpad_down_pin = remapping.pin as usize,
                DpedalInput::DpadLeft => dpad_left_pin = remapping.pin as usize,
                DpedalInput::DpadRight => dpad_right_pin = remapping.pin as usize,
                DpedalInput::ButtonLeft => button_left_pin = remapping.pin as usize,
                DpedalInput::ButtonRight => button_right_pin = remapping.pin as usize,
            }
        }

        let button_left = input(self.pins[button_left_pin].take().unwrap());
        let button_right = input(self.pins[button_right_pin].take().unwrap());
        let dpad_up = input(self.pins[dpad_up_pin].take().unwrap());
        let dpad_down = input(self.pins[dpad_down_pin].take().unwrap());
        let dpad_left = input(self.pins[dpad_left_pin].take().unwrap());
        let dpad_right = input(self.pins[dpad_right_pin].take().unwrap());

        let mut state = State::new();
        let mut loaded_profile_index: u8 = 0;

        'main_loop: loop {
            // Detect config updates (ReloadConfig from web configurator); reset to profile 0
            if rx.try_changed().is_some() {
                state = State::new();
                loaded_profile_index = u8::MAX; // force reload via the block below
            }

            // Load new profile from flash when current profile changed
            if state.current_profile != loaded_profile_index {
                *current_profile = self
                    .config_flash
                    .lock()
                    .await
                    .load_profile(state.current_profile)
                    .await;
                loaded_profile_index = state.current_profile;
                state.mapping_states.clear();
            }

            let input_state = DpedalInputState {
                button_left: button_left.is_low(),
                button_right: button_right.is_low(),
                dpad_up: dpad_up.is_low(),
                dpad_down: dpad_down.is_low(),
                dpad_left: dpad_left.is_low(),
                dpad_right: dpad_right.is_low(),
            };

            // Restore mapping state to full length in case it was cleared earlier
            while current_profile.mappings.len() > state.mapping_states.len() {
                if state.mapping_states.push(MappingState::new()).is_err() {
                    defmt::panic!("mapping state overflow");
                }
            }

            for (i, mapping) in current_profile.mappings.iter().enumerate() {
                let all_pressed = input_state.is_all_pressed(&mapping.input_set);
                state.mapping_states[i] = match state.mapping_states[i]
                    .process(mapping, all_pressed, &mut state)
                    .await
                {
                    ControlFlow::Continue(ms) => ms,
                    ControlFlow::Break(()) => continue 'main_loop,
                };
            }

            Timer::after_millis(1).await;
        }
    }
}

pub(crate) struct State {
    /// The index of the currently selected profile
    pub(crate) current_profile: u8,
    /// Tracks press/release state for each mapping in the current profile
    pub(crate) mapping_states: Vec<MappingState, MAX_MAPPINGS>,
}

impl State {
    fn new() -> Self {
        State {
            current_profile: 0,
            mapping_states: Vec::new(),
        }
    }
}

struct DpedalInputState {
    button_left: bool,
    button_right: bool,
    dpad_up: bool,
    dpad_down: bool,
    dpad_left: bool,
    dpad_right: bool,
}

impl DpedalInputState {
    fn is_all_pressed(&self, check: &[DpedalInput]) -> bool {
        // Disable the mapping when the inputs are entirely empty
        // It is an obvious configuration mistake and having it constantly trigger the input would be very annoying
        if check.is_empty() {
            return false;
        }

        for input in check {
            let pressed = match input {
                DpedalInput::DpadUp => self.dpad_up,
                DpedalInput::DpadDown => self.dpad_down,
                DpedalInput::DpadLeft => self.dpad_left,
                DpedalInput::DpadRight => self.dpad_right,
                DpedalInput::ButtonLeft => self.button_left,
                DpedalInput::ButtonRight => self.button_right,
            };

            if !pressed {
                return false;
            }
        }
        true
    }
}

// TODO: become Input::new
fn input<T: PeripheralType + Pin>(pin: Peri<'static, T>) -> Input<'static> {
    let mut pin = Input::new(pin, Pull::Up);
    pin.set_schmitt(true);
    pin
}
