use core::ops::ControlFlow;

use crate::config::{CONFIG, CONFIG_UPDATED};
use crate::keyboard::{KEYBOARD_CHANNEL, KeyboardEvent};
use crate::mouse::{MOUSE_CHANNEL, MouseEvent};
use arrayvec::ArrayVec;
use dpedal_config::{
    ComputerInput, DPedalControl, DpedalInput, MAX_MAPPINGS, MAX_PROFILES, Profile,
};
use embassy_rp::gpio::{AnyPin, Input, Pin, Pull};
use embassy_rp::{Peri, PeripheralType};
use embassy_time::Timer;
use static_cell::StaticCell;

static PROFILES: StaticCell<ArrayVec<Profile, MAX_PROFILES>> = StaticCell::new();

pub struct Inputs {
    pins: [Option<Peri<'static, AnyPin>>; 30],
}

impl Inputs {
    pub fn new(pins: [Option<Peri<'static, AnyPin>>; 30]) -> Self {
        Inputs { pins }
    }

    pub async fn process(&mut self) {
        let mut rx = CONFIG_UPDATED.receiver().unwrap();

        let mut button_left_pin = 13;
        let mut button_right_pin = 27;
        let mut dpad_up_pin = 26;
        let mut dpad_down_pin = 16;
        let mut dpad_left_pin = 17;
        let mut dpad_right_pin = 22;

        let profiles = {
            // Wait for initial config load
            rx.get().await;
            let config = CONFIG.lock().await;
            let config = config.as_ref().unwrap();

            // pin_remappings cant be set by the web configurator, so we dont need to worry about resetting this after web configuration occurs.
            for remapping in &config.pin_remappings {
                match remapping.input {
                    DpedalInput::DpadUp => dpad_up_pin = remapping.pin as usize,
                    DpedalInput::DpadDown => dpad_down_pin = remapping.pin as usize,
                    DpedalInput::DpadLeft => dpad_left_pin = remapping.pin as usize,
                    DpedalInput::DpadRight => dpad_right_pin = remapping.pin as usize,
                    DpedalInput::ButtonLeft => button_left_pin = remapping.pin as usize,
                    DpedalInput::ButtonRight => button_right_pin = remapping.pin as usize,
                }
            }

            PROFILES.init(config.profiles.clone())
        };

        let button_left = input(self.pins[button_left_pin].take().unwrap());
        let button_right = input(self.pins[button_right_pin].take().unwrap());
        let dpad_up = input(self.pins[dpad_up_pin].take().unwrap());
        let dpad_down = input(self.pins[dpad_down_pin].take().unwrap());
        let dpad_left = input(self.pins[dpad_left_pin].take().unwrap());
        let dpad_right = input(self.pins[dpad_right_pin].take().unwrap());

        let mut state = State::new();
        'main_loop: loop {
            // Detect config changes and update local config + clear state
            if rx.try_changed().is_some() {
                *profiles = CONFIG.lock().await.as_ref().unwrap().profiles.clone();
                state = State::new();
            }

            if let Some(profile) = profiles.get(state.current_profile as usize) {
                let input_state = DpedalInputState {
                    button_left: button_left.is_low(),
                    button_right: button_right.is_low(),
                    dpad_up: dpad_up.is_low(),
                    dpad_down: dpad_down.is_low(),
                    dpad_left: dpad_left.is_low(),
                    dpad_right: dpad_right.is_low(),
                };

                // Restore mapping state to full length in case it was cleared earlier
                while profile.mappings.len() > state.mapping_state.len() {
                    state.mapping_state.push(MappingState::Released);
                }

                for (i, mapping) in profile.mappings.iter().enumerate() {
                    if input_state.is_all_pressed(&mapping.input_set) {
                        for output in &mapping.output_sequence {
                            Inputs::pressed(*output).await;
                        }
                        state.mapping_state[i] = MappingState::Pressed;
                    } else {
                        for output in &mapping.output_sequence {
                            if let MappingState::Pressed = state.mapping_state[i]
                                && let ControlFlow::Break(()) =
                                    Inputs::released(*output, &mut state).await
                            {
                                continue 'main_loop;
                            }
                        }
                        state.mapping_state[i] = MappingState::Released;
                    }
                }
            } else {
                defmt::error!("No profile with index {}", state.current_profile)
            }
            Timer::after_millis(1).await;
        }
    }

    async fn pressed(input: ComputerInput) {
        match input {
            ComputerInput::Keyboard(key) => {
                KEYBOARD_CHANNEL.send(KeyboardEvent::Pressed(key)).await
            }
            ComputerInput::Mouse(mouse) => MOUSE_CHANNEL.send(MouseEvent::Pressed(mouse)).await,
            ComputerInput::Control(_) => {}
        }
    }

    // Returns Break when state is invalidated and we need to start the next loop
    async fn released(input: ComputerInput, state: &mut State) -> ControlFlow<()> {
        match input {
            ComputerInput::Keyboard(key) => {
                KEYBOARD_CHANNEL.send(KeyboardEvent::Released(key)).await
            }
            ComputerInput::Mouse(mouse) => MOUSE_CHANNEL.send(MouseEvent::Released(mouse)).await,
            ComputerInput::Control(control) => state.update(control)?,
        }

        ControlFlow::Continue(())
    }
}

struct State {
    /// The index of the currently selected profile
    current_profile: u8,
    /// Tracks press/release state for each mapping in the current profile
    mapping_state: ArrayVec<MappingState, MAX_MAPPINGS>,
}

impl State {
    fn new() -> Self {
        State {
            current_profile: 0,
            mapping_state: ArrayVec::new(),
        }
    }

    fn update(&mut self, event: DPedalControl) -> ControlFlow<()> {
        match event {
            // TODO: implement these controls
            DPedalControl::AfterMillisHold(_)
            | DPedalControl::AfterMillisRelease(_)
            | DPedalControl::Restart => ControlFlow::Continue(()),
            DPedalControl::SetProfile(profile) => {
                self.current_profile = profile;
                self.mapping_state.clear();
                ControlFlow::Break(())
            }
        }
    }
}

enum MappingState {
    Pressed,
    Released,
    // TODO
    //MacroStuff,
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
