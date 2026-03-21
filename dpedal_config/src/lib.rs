#![no_std]

pub mod web_config_protocol;

// Memory layout
pub const RP2040_FLASH_OFFSET: usize = 0x10000000;
pub const PICO_FLASH_SIZE: usize = 1024 * 1024 * 2; // 2 MiB

pub const FIRMWARE_OFFSET: usize = 0;
pub const FIRMWARE_SIZE: usize = 1024 * 1024; // 1 MiB
pub const CONFIG_OFFSET: usize = 1024 * 1024; // 1 MiB
/// How much space in flash is available for storing config (handed to MapStorage for wear leveling)
pub const CONFIG_AVAILABLE_SIZE: usize = 1024 * 1024; // 1 MiB
pub const CONFIG_FLASH_RANGE: core::ops::Range<u32> =
    CONFIG_OFFSET as u32..(CONFIG_OFFSET as u32 + CONFIG_AVAILABLE_SIZE as u32);
/// How much space a single serialized Config object takes up in flash
pub const CONFIG_SINGLE_SIZE: usize = 1024 * 12; // 12 KiB

use defmt::Format;
use heapless::{String, Vec};
use serde::{Deserialize, Serialize};
use strum::{EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

const fn assert_config_fits_in_flash() {
    // TODO: This isnt actually accurate, since the data will be serialized into postcard format first.
    //       Is there a way to calculate the maximum possible size in postcard format?
    assert!(core::mem::size_of::<Config>() <= CONFIG_SINGLE_SIZE);
}

const fn assert_config_size_fits_into_writable_flash_blocks() {
    // Flash can only be written in blocks of 4096 bytes.
    assert!(CONFIG_OFFSET.is_multiple_of(4096));
    assert!(CONFIG_AVAILABLE_SIZE.is_multiple_of(4096));
}

const _: () = assert_config_fits_in_flash();
const _: () = assert_config_size_fits_into_writable_flash_blocks();

pub const MAX_PROFILES: usize = 5;
pub const MAX_NICKNAME_LEN: usize = 50;
pub const MAX_PIN_REMAPPINGS: usize = 6;

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Config {
    pub version: u32,
    pub nickname: String<MAX_NICKNAME_LEN>,
    pub device: Device,
    pub color: u32,
    pub profiles: Vec<Profile, MAX_PROFILES>,
    pub pin_remappings: Vec<PinRemapping, MAX_PIN_REMAPPINGS>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: Default::default(),
            nickname: String::try_from("my DPedal").unwrap(),
            device: Default::default(),
            color: 0x1790e3,
            profiles: Vec::from_iter([Profile::default()]),
            pin_remappings: Default::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Default, Clone, IntoStaticStr)]
pub enum Device {
    #[default]
    DpedalV3,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Default, Clone)]
pub struct PinRemapping {
    pub input: DpedalInput,
    // TODO: make u8
    pub pin: u32,
}

pub const MAX_MAPPINGS: usize = 12;
pub const MAX_COMPUTER_INPUTS: usize = 25;
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Profile {
    pub mappings: Vec<Mapping, MAX_MAPPINGS>,
}

impl Profile {
    pub fn empty() -> Self {
        Self {
            mappings: Vec::new(),
        }
    }

    pub fn mouse_move() -> Self {
        Self {
            mappings: Vec::from_iter([
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::DpadLeft]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Mouse(MouseInput::MoveLeft(
                        300,
                    ))]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::DpadRight]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Mouse(MouseInput::MoveRight(
                        300,
                    ))]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::DpadUp]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Mouse(MouseInput::MoveUp(
                        300,
                    ))]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::DpadDown]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Mouse(MouseInput::MoveDown(
                        300,
                    ))]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::ButtonLeft]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Mouse(MouseInput::ClickLeft)]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::ButtonRight]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Mouse(MouseInput::ClickRight)]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::ButtonRight]),
                    mode: MappingMode::OnHold { hold_ms: 2000 },
                    output_sequence: Vec::from_iter([ComputerInput::Control(
                        DPedalControl::SetProfile(0),
                    )]),
                },
            ]),
        }
    }

    pub fn arrow_keys() -> Self {
        Self {
            mappings: Vec::from_iter([
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::DpadLeft]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Keyboard(
                        KeyboardInput::LeftArrow,
                    )]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::DpadRight]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Keyboard(
                        KeyboardInput::RightArrow,
                    )]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::DpadUp]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Keyboard(
                        KeyboardInput::UpArrow,
                    )]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::DpadDown]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Keyboard(
                        KeyboardInput::DownArrow,
                    )]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::ButtonLeft]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([
                        ComputerInput::Keyboard(KeyboardInput::LeftShift),
                        ComputerInput::Keyboard(KeyboardInput::Tab),
                    ]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::ButtonRight]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Keyboard(KeyboardInput::Tab)]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::ButtonRight]),
                    mode: MappingMode::OnHold { hold_ms: 2000 },
                    output_sequence: Vec::from_iter([ComputerInput::Control(
                        DPedalControl::SetProfile(0),
                    )]),
                },
            ]),
        }
    }

    pub fn macro_demo() -> Self {
        Self {
            mappings: Vec::from_iter([
                // Left button: move mouse in a 200px square (at 200px/s, each side takes 1000ms)
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::ButtonLeft]),
                    mode: MappingMode::MacroOnPress,
                    output_sequence: Vec::from_iter([
                        ComputerInput::Mouse(MouseInput::MoveRight(500)),
                        ComputerInput::Control(DPedalControl::AfterMillisRelease(500)),
                        ComputerInput::Mouse(MouseInput::MoveDown(500)),
                        ComputerInput::Control(DPedalControl::AfterMillisRelease(500)),
                        ComputerInput::Mouse(MouseInput::MoveLeft(500)),
                        ComputerInput::Control(DPedalControl::AfterMillisRelease(500)),
                        ComputerInput::Mouse(MouseInput::MoveUp(500)),
                        ComputerInput::Control(DPedalControl::AfterMillisRelease(500)),
                    ]),
                },
                // Right button: Ctrl+L, type "dpedal.com", Enter
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::ButtonRight]),
                    mode: MappingMode::MacroOnPress,
                    output_sequence: Vec::from_iter([
                        ComputerInput::Keyboard(KeyboardInput::LeftControl),
                        ComputerInput::Keyboard(KeyboardInput::L),
                        ComputerInput::Control(DPedalControl::AfterMillisRelease(100)),
                        ComputerInput::Keyboard(KeyboardInput::D),
                        ComputerInput::Control(DPedalControl::AfterMillisRelease(50)),
                        ComputerInput::Keyboard(KeyboardInput::P),
                        ComputerInput::Control(DPedalControl::AfterMillisRelease(50)),
                        ComputerInput::Keyboard(KeyboardInput::E),
                        ComputerInput::Control(DPedalControl::AfterMillisRelease(50)),
                        ComputerInput::Keyboard(KeyboardInput::D),
                        ComputerInput::Control(DPedalControl::AfterMillisRelease(50)),
                        ComputerInput::Keyboard(KeyboardInput::A),
                        ComputerInput::Control(DPedalControl::AfterMillisRelease(50)),
                        ComputerInput::Keyboard(KeyboardInput::L),
                        ComputerInput::Control(DPedalControl::AfterMillisRelease(50)),
                        ComputerInput::Keyboard(KeyboardInput::PeriodGreaterThan),
                        ComputerInput::Control(DPedalControl::AfterMillisRelease(50)),
                        ComputerInput::Keyboard(KeyboardInput::C),
                        ComputerInput::Control(DPedalControl::AfterMillisRelease(50)),
                        ComputerInput::Keyboard(KeyboardInput::O),
                        ComputerInput::Control(DPedalControl::AfterMillisRelease(50)),
                        ComputerInput::Keyboard(KeyboardInput::M),
                        ComputerInput::Control(DPedalControl::AfterMillisRelease(50)),
                        ComputerInput::Keyboard(KeyboardInput::Enter),
                    ]),
                },
            ]),
        }
    }
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            mappings: Vec::from_iter([
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::DpadLeft]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Mouse(
                        MouseInput::ScrollLeft(20),
                    )]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::DpadRight]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Mouse(
                        MouseInput::ScrollRight(20),
                    )]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::DpadUp]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Mouse(MouseInput::ScrollUp(
                        20,
                    ))]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::DpadDown]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Mouse(
                        MouseInput::ScrollDown(20),
                    )]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::ButtonLeft]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Keyboard(
                        KeyboardInput::PageUp,
                    )]),
                },
                Mapping {
                    input_set: Vec::from_iter([DpedalInput::ButtonRight]),
                    mode: MappingMode::OnPress,
                    output_sequence: Vec::from_iter([ComputerInput::Keyboard(
                        KeyboardInput::PageDown,
                    )]),
                },
            ]),
        }
    }
}

pub const MAX_DPEDAL_INPUTS: usize = 4;
#[derive(Serialize, Deserialize, Debug, PartialEq, Default, Clone)]
pub struct Mapping {
    /// The input_set produces:
    /// * a press event when (the last event was release OR there have been no events yet) AND all DpedalInput are held
    /// * a release event when the last event was press AND at least one DPedalInput is not held.
    pub input_set: Vec<DpedalInput, MAX_DPEDAL_INPUTS>,

    /// The output_sequence is triggered and/or terminated when the specified conditions in the input_set are met.
    pub mode: MappingMode,

    /// A sequence of ComputerInputs forming a simple key press or macro.
    /// Can contain just a single ComputerInput for use as a regular computer input instead of a macro.
    /// Once triggered, the output_sequence runs till completion or the terminate_on condition is met.
    pub output_sequence: Vec<ComputerInput, MAX_COMPUTER_INPUTS>,
}

#[derive(
    Format, Serialize, Deserialize, Debug, PartialEq, Default, Clone, Copy, EnumIter, IntoStaticStr,
)]
pub enum MappingMode {
    /// The output_sequence is triggered on input_set press
    /// The output_sequence is terminated on input_set release or the output_sequence reaches its end
    #[default]
    OnPress,

    /// The output_sequence is triggered on an input_set hold that lasts longer than hold_ms
    /// The output_sequence is terminated on input_set release or the output_sequence reaches its end
    OnHold { hold_ms: u16 },

    /// The output_sequence is triggered on an input_set press
    /// The output_sequence is terminated on the next input_set press
    Toggle,

    /// The output_sequence is triggered on input_set press
    /// The output_sequence is terminated when the output_sequence reaches its end
    /// input_set events that occur while the output_sequence is still running have no effect
    MacroOnPress,

    /// The output_sequence is triggered on input_set release
    /// The output_sequence is terminated when the output_sequence reaches its end
    /// input_set events that occur while the output_sequence is still running have no effect
    MacroOnRelease,

    /// The output_sequence is triggered when an input_set press and then release occur within tap_ms
    /// The output_sequence is terminated when the output_sequence reaches its end
    /// input_set events that occur while the output_sequence is still running have no effect
    MacroOnTap { tap_ms: u16 },

    /// The output_sequence is triggered when an input_set press and then release and then press and then release occurs within tap_ms
    /// The output_sequence is terminated when the output_sequence reaches its end
    /// input_set events that occur while the output_sequence is still running have no effect
    MacroOnDoubleTap { tap_ms: u16 },

    /// The output_sequence is triggered on an input_set hold that lasts longer than hold_ms
    /// The output_sequence is terminated when the output_sequence reaches its end
    /// input_set events that occur while the output_sequence is still running have no effect
    MacroOnHold { hold_ms: u16 },
}

impl MappingMode {
    pub fn is_macro(&self) -> bool {
        matches!(
            self,
            MappingMode::MacroOnPress
                | MappingMode::MacroOnRelease
                | MappingMode::MacroOnTap { .. }
                | MappingMode::MacroOnDoubleTap { .. }
                | MappingMode::MacroOnHold { .. }
        )
    }

    pub fn from_string(s: &str, value: &str) -> Option<Self> {
        match s {
            "OnPress" => Some(MappingMode::OnPress),
            "OnHold" => Some(MappingMode::OnHold {
                hold_ms: value.parse().ok()?,
            }),
            "Toggle" => Some(MappingMode::Toggle),
            "MacroOnPress" => Some(MappingMode::MacroOnPress),
            "MacroOnRelease" => Some(MappingMode::MacroOnRelease),
            "MacroOnTap" => Some(MappingMode::MacroOnTap {
                tap_ms: value.parse().ok()?,
            }),
            "MacroOnDoubleTap" => Some(MappingMode::MacroOnDoubleTap {
                tap_ms: value.parse().ok()?,
            }),
            "MacroOnHold" => Some(MappingMode::MacroOnHold {
                hold_ms: value.parse().ok()?,
            }),
            _ => None,
        }
    }
}

#[derive(
    Format, Serialize, Deserialize, Debug, PartialEq, Default, Clone, Copy, EnumIter, IntoStaticStr,
)]
pub enum DpedalInput {
    #[default]
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    ButtonLeft,
    ButtonRight,
}

impl DpedalInput {
    pub fn from_string(s: &str) -> Option<Self> {
        match s {
            "DpadUp" => Some(Self::DpadUp),
            "DpadDown" => Some(Self::DpadDown),
            "DpadLeft" => Some(Self::DpadLeft),
            "DpadRight" => Some(Self::DpadRight),
            "ButtonLeft" => Some(Self::ButtonLeft),
            "ButtonRight" => Some(Self::ButtonRight),
            _ => None,
        }
    }

    pub fn from_string_kebab(s: &str) -> Option<Self> {
        match s {
            "dpad-up" => Some(Self::DpadUp),
            "dpad-down" => Some(Self::DpadDown),
            "dpad-left" => Some(Self::DpadLeft),
            "dpad-right" => Some(Self::DpadRight),
            "button-left" => Some(Self::ButtonLeft),
            "button-right" => Some(Self::ButtonRight),
            _ => None,
        }
    }
}

#[derive(Format, Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum ComputerInput {
    Mouse(MouseInput),
    Keyboard(KeyboardInput),
    Control(DPedalControl),
}

#[derive(
    Format, Serialize, Deserialize, Debug, PartialEq, Default, Clone, Copy, EnumIter, IntoStaticStr,
)]
pub enum MouseInput {
    /// The mouse will scroll up by this many pixels per second
    ScrollUp(i16),
    /// The mouse will scroll down by this many pixels per second
    ScrollDown(i16),
    /// The mouse will scroll right by this many pixels per second
    ScrollRight(i16),
    /// The mouse will scroll left by this many pixels per second
    ScrollLeft(i16),
    /// The cursor will move up by this many pixels per second
    MoveUp(i16),
    /// The cursor will move down by this many pixels per second
    MoveDown(i16),
    /// The cursor will move right by this many pixels per second
    MoveRight(i16),
    /// The cursor will move left by this many pixels per second
    MoveLeft(i16),
    #[default]
    ClickLeft,
    ClickMiddle,
    ClickRight,
}

impl MouseInput {
    pub fn from_string(s: &str, value: &str) -> Option<Self> {
        match s {
            "ScrollUp" | "scroll-up" => Some(MouseInput::ScrollUp(value.parse().ok()?)),
            "ScrollDown" | "scroll-down" => Some(MouseInput::ScrollDown(value.parse().ok()?)),
            "ScrollRight" | "scroll-right" => Some(MouseInput::ScrollRight(value.parse().ok()?)),
            "ScrollLeft" | "scroll-left" => Some(MouseInput::ScrollLeft(value.parse().ok()?)),
            "MoveUp" | "move-up" => Some(MouseInput::MoveUp(value.parse().ok()?)),
            "MoveDown" | "move-down" => Some(MouseInput::MoveDown(value.parse().ok()?)),
            "MoveRight" | "move-right" => Some(MouseInput::MoveRight(value.parse().ok()?)),
            "MoveLeft" | "move-left" => Some(MouseInput::MoveLeft(value.parse().ok()?)),
            "ClickLeft" | "click-left" => Some(MouseInput::ClickLeft),
            "ClickMiddle" | "click-middle " => Some(MouseInput::ClickMiddle),
            "ClickRight" | "click-right" => Some(MouseInput::ClickRight),
            _ => None,
        }
    }
}

#[derive(
    Format, Serialize, Deserialize, Debug, PartialEq, Default, Clone, Copy, EnumIter, EnumString,
)]
pub enum KeyboardInput {
    #[default]
    /// Keyboard a and A (Footnote 2)
    A = 0x04,
    /// Keyboard b and B
    B = 0x05,
    /// Keyboard c and C (Footnote 2)
    C = 0x06,
    /// Keyboard d and D
    D = 0x07,
    /// Keyboard e and E
    E = 0x08,
    /// Keyboard f and F
    F = 0x09,
    /// Keyboard g and G
    G = 0x0A,
    /// Keyboard h and H
    H = 0x0B,
    /// Keyboard i and I
    I = 0x0C,
    /// Keyboard j and J
    J = 0x0D,
    /// Keyboard k and K
    K = 0x0E,
    /// Keyboard l and L
    L = 0x0F,
    /// Keyboard m and M (Footnote 2)
    M = 0x10,
    /// Keyboard n and N
    N = 0x11,
    /// Keyboard o and O (Footnote 2)
    O = 0x12,
    /// Keyboard p and P (Footnote 2)
    P = 0x13,
    /// Keyboard q and Q (Footnote 2)
    Q = 0x14,
    /// Keyboard r and R
    R = 0x15,
    /// Keyboard s and S
    S = 0x16,
    /// Keyboard t and T
    T = 0x17,
    /// Keyboard u and U
    U = 0x18,
    /// Keyboard v and V
    V = 0x19,
    /// Keyboard w and W (Footnote 2)
    W = 0x1A,
    /// Keyboard x and X (Footnote 2)
    X = 0x1B,
    /// Keyboard y and Y (Footnote 2)
    Y = 0x1C,
    /// Keyboard z and Z (Footnote 2)
    Z = 0x1D,
    /// Keyboard 1 and ! (Footnote 2)
    TopRow1Exclamation = 0x1E,
    /// Keyboard 2 and @ (Footnote 2)
    TopRow2At = 0x1F,
    /// Keyboard 3 and # (Footnote 2)
    TopRow3Hash = 0x20,
    /// Keyboard 4 and $ (Footnote 2)
    TopRow4Dollar = 0x21,
    /// Keyboard 5 and % (Footnote 2)
    TopRow5Percent = 0x22,
    /// Keyboard 6 and ^ (Footnote 2)
    TopRow6Caret = 0x23,
    /// Keyboard 7 and & (Footnote 2)
    TopRow7Ampersand = 0x24,
    /// Keyboard 8 and * (Footnote 2)
    TopRow8Asterisk = 0x25,
    /// Keyboard 9 and ( (Footnote 2)
    TopRow9OpenParens = 0x26,
    /// Keyboard 0 and ) (Footnote 2)
    TopRow0CloseParens = 0x27,
    /// Keyboard Return (ENTER) (Footnote 3)
    ///  (Footnote 3): Keyboard Enter and Keypad Enter generate different Usage codes.
    Enter = 0x28,
    /// Keyboard ESCAPE
    Escape = 0x29,
    /// Keyboard DELETE (Backspace) (Footnote 4)
    Backspace = 0x2A,
    /// Keyboard Tab
    Tab = 0x2B,
    /// Keyboard Spacebar
    Spacebar = 0x2C,
    /// Keyboard - and _ (Footnote 2)
    DashUnderscore = 0x2D,
    /// Keyboard = and + (Footnote 2)
    EqualPlus = 0x2E,
    /// Keyboard [ and { (Footnote 2)
    OpenBracketBrace = 0x2F,
    /// Keyboard ] and } (Footnote 2)
    CloseBracketBrace = 0x30,
    /// Keyboard \ and |
    BackslashBar = 0x31,
    /// Keyboard Non-US # and (Footnote 5)
    NonUSHash = 0x32,
    /// Keyboard ; and : (Footnote 2)
    SemiColon = 0x33,
    /// Keyboard ' and " (Footnote 2)
    SingleDoubleQuote = 0x34,
    /// Keyboard ` and ~ (Footnote 2)
    BacktickTilde = 0x35,
    /// Keyboard , and < (Footnote 2)
    CommaLessThan = 0x36,
    /// Keyboard . and > (Footnote 2)
    PeriodGreaterThan = 0x37,
    /// Keyboard / and ? (Footnote 2)
    SlashQuestion = 0x38,
    /// Keyboard Caps Lock (Footnote 6)
    CapsLock = 0x39,
    /// Keyboard F1
    F1 = 0x3A,
    /// Keyboard F2
    F2 = 0x3B,
    /// Keyboard F3
    F3 = 0x3C,
    /// Keyboard F4
    F4 = 0x3D,
    /// Keyboard F5
    F5 = 0x3E,
    /// Keyboard F6
    F6 = 0x3F,
    /// Keyboard F7
    F7 = 0x40,
    /// Keyboard F8
    F8 = 0x41,
    /// Keyboard F9
    F9 = 0x42,
    /// Keyboard F10
    F10 = 0x43,
    /// Keyboard F11
    F11 = 0x44,
    /// Keyboard F12
    F12 = 0x45,
    /// Keyboard PrintScreen (Footnote 7)
    PrintScreen = 0x46,
    /// Keyboard ScrollLock (Footnote 6)
    ScrollLock = 0x47,
    /// Keyboard Pause (Footnote 7)
    Pause = 0x48,
    /// Keyboard Insert (Footnote 7)
    Insert = 0x49,
    /// Keyboard Home (Footnote 7)
    Home = 0x4A,
    /// Keyboard PageUp (Footnote 7)
    PageUp = 0x4B,
    /// Keyboard Delete Forward (Footnote 7) (Footnote 8)
    Delete = 0x4C,
    /// Keyboard End (Footnote 7)
    End = 0x4D,
    /// Keyboard PageDown (Footnote 7)
    PageDown = 0x4E,
    /// Keyboard RightArrow (Footnote 7)
    RightArrow = 0x4F,
    /// Keyboard LeftArrow (Footnote 7)
    LeftArrow = 0x50,
    /// Keyboard DownArrow (Footnote 7)
    DownArrow = 0x51,
    /// Keyboard UpArrow (Footnote 7)
    UpArrow = 0x52,
    /// Keypad Num Lock and Clear (Footnote 6)
    KeypadNumLock = 0x53,
    /// Keypad / (Footnote 7)
    KeypadDivide = 0x54,
    /// Keypad *
    KeypadMultiply = 0x55,
    /// Keypad -
    KeypadMinus = 0x56,
    /// Keypad +
    KeypadPlus = 0x57,
    /// Keypad ENTER (Footnote 3)
    KeypadEnter = 0x58,
    /// Keypad 1 and End
    Keypad1End = 0x59,
    /// Keypad 2 and DownArrow
    Keypad2DownArrow = 0x5A,
    /// Keypad 3 and PageDown
    Keypad3PageDown = 0x5B,
    /// Keypad 4 and LeftArrow
    Keypad4LeftArrow = 0x5C,
    /// Keypad 5
    Keypad5 = 0x5D,
    /// Keypad 6 and RightArrow
    Keypad6RightArrow = 0x5E,
    /// Keypad 7 and Home
    Keypad7Home = 0x5F,
    /// Keypad 8 and UpArrow
    Keypad8UpArrow = 0x60,
    /// Keypad 9 and PageUp
    Keypad9PageUp = 0x61,
    /// Keypad 0 and Insert
    Keypad0Insert = 0x62,
    /// Keypad . and Delete
    KeypadPeriodDelete = 0x63,
    /// Keyboard Non-US \ and | (Footnote 9) (Footnote 10)
    NonUSSlash = 0x64,
    /// Keyboard Application (Footnote 11)
    Application = 0x65,
    /// Keyboard Power (Footnote 1)
    Power = 0x66,
    /// Keypad =
    KeypadEqual = 0x67,
    /// Keyboard F13
    F13 = 0x68,
    /// Keyboard F14
    F14 = 0x69,
    /// Keyboard F15
    F15 = 0x6A,
    /// Keyboard F16
    F16 = 0x6B,
    /// Keyboard F17
    F17 = 0x6C,
    /// Keyboard F18
    F18 = 0x6D,
    /// Keyboard F19
    F19 = 0x6E,
    /// Keyboard F20
    F20 = 0x6F,
    /// Keyboard F21
    F21 = 0x70,
    /// Keyboard F22
    F22 = 0x71,
    /// Keyboard F23
    F23 = 0x72,
    /// Keyboard F24
    F24 = 0x73,
    /// Keyboard Execute
    Execute = 0x74,
    /// Keyboard Help
    Help = 0x75,
    /// Keyboard Menu
    Menu = 0x76,
    /// Keyboard Select
    Select = 0x77,
    /// Keyboard Stop
    Stop = 0x78,
    /// Keyboard Again
    Again = 0x79,
    /// Keyboard Undo
    Undo = 0x7A,
    /// Keyboard Cut
    Cut = 0x7B,
    /// Keyboard Copy
    Copy = 0x7C,
    /// Keyboard Paste
    Paste = 0x7D,
    /// Keyboard Find
    Find = 0x7E,
    /// Keyboard Mute
    Mute = 0x7F,
    /// Keyboard Volume Up
    VolumeUp = 0x80,
    /// Keyboard Volume Down
    VolumeDown = 0x81,
    /// Keyboad Locking Caps Lock (Footnote 12)
    LockingCapsLock = 0x82,
    /// Keyboad Locking Num Lock (Footnote 12)
    LockingNumLock = 0x83,
    /// Keyboad Locking Scroll Lock (Footnote 12)
    LockingScrollLock = 0x84,
    /// Keypad Comma (Footnote 13)
    KeypadComma = 0x85,
    /// Keypad Equal Sign (Footnote 14)
    KeypadEqualSign = 0x86,
    /// Keyboard International1 (Footnote 15) (Footnote 16)
    International1 = 0x87,
    /// Keyboard International2 (Footnote 17)
    International2 = 0x88,
    /// Keyboard International3 (Footnote 18)
    International3 = 0x89,
    /// Keyboard International4 (Footnote 19)
    International4 = 0x8A,
    /// Keyboard International5 (Footnote 20)
    International5 = 0x8B,
    /// Keyboard International6 (Footnote 21)
    International6 = 0x8C,
    /// Keyboard International7 (Footnote 22)
    International7 = 0x8D,
    /// Keyboard International8 (Footnote 23)
    International8 = 0x8E,
    /// Keyboard International9 (Footnote 23)
    International9 = 0x8F,
    /// Keyboard LANG1 (Footnote 24)
    LANG1 = 0x90,
    /// Keyboard LANG2 (Footnote 25)
    LANG2 = 0x91,
    /// Keyboard LANG3 (Footnote 26)
    LANG3 = 0x92,
    /// Keyboard LANG4 (Footnote 27)
    LANG4 = 0x93,
    /// Keyboard LANG5 (Footnote 28)
    LANG5 = 0x94,
    /// Keyboard LANG6 (Footnote 29)
    LANG6 = 0x95,
    /// Keyboard LANG7 (Footnote 29)
    LANG7 = 0x96,
    /// Keyboard LANG8 (Footnote 29)
    LANG8 = 0x97,
    /// Keyboard LANG9 (Footnote 29)
    LANG9 = 0x98,
    /// Keyboard Alternate Erase (Footnote 30)
    AlternateErase = 0x99,
    /// Keyboard SysReq/Attention (Footnote 7)
    SysReqAttention = 0x9A,
    /// Keyboard Cancel
    Cancel = 0x9B,
    /// Keyboard Clear
    Clear = 0x9C,
    /// Keyboard Prior
    Prior = 0x9D,
    /// Keyboard Return
    Return = 0x9E,
    /// Keyboard Separator
    Separator = 0x9F,
    /// Keyboard Out
    Out = 0xA0,
    /// Keyboard Oper
    Oper = 0xA1,
    /// Keyboard Clear/Again
    ClearAgain = 0xA2,
    /// Keyboard CrSel/Props
    CrSelProps = 0xA3,
    /// Keyboard ExSel
    ExSel = 0xA4,
    /// Keyboard LeftControl
    LeftControl = 0xE0,
    /// Keyboard LeftShift
    LeftShift = 0xE1,
    /// Keyboard LeftAlt
    LeftAlt = 0xE2,
    /// Keyboard LeftGUI (Footnote 11) (Footnote 33)
    LeftWindows = 0xE3,
    /// Keyboard RightControl
    RightControl = 0xE4,
    /// Keyboard RightShift
    RightShift = 0xE5,
    /// Keyboard RightAlt
    RightAlt = 0xE6,
    /// Keyboard RightGUI (Footnote 11) (Footnote 34)
    RightWindows = 0xE7,
}

impl KeyboardInput {
    pub fn common_iter() -> impl Iterator<Item = Self> {
        COMMON_KEYBOARD_INPUTS.into_iter()
    }

    pub fn obscure_iter() -> impl Iterator<Item = Self> {
        Self::iter().filter(|x| !COMMON_KEYBOARD_INPUTS.contains(x))
    }
}

const COMMON_KEYBOARD_INPUTS: [KeyboardInput; 93] = [
    KeyboardInput::RightArrow,
    KeyboardInput::LeftArrow,
    KeyboardInput::DownArrow,
    KeyboardInput::UpArrow,
    KeyboardInput::PageUp,
    KeyboardInput::PageDown,
    KeyboardInput::Tab,
    KeyboardInput::Escape,
    KeyboardInput::A,
    KeyboardInput::B,
    KeyboardInput::C,
    KeyboardInput::D,
    KeyboardInput::E,
    KeyboardInput::F,
    KeyboardInput::G,
    KeyboardInput::H,
    KeyboardInput::I,
    KeyboardInput::J,
    KeyboardInput::K,
    KeyboardInput::L,
    KeyboardInput::M,
    KeyboardInput::N,
    KeyboardInput::O,
    KeyboardInput::P,
    KeyboardInput::Q,
    KeyboardInput::R,
    KeyboardInput::S,
    KeyboardInput::T,
    KeyboardInput::U,
    KeyboardInput::V,
    KeyboardInput::W,
    KeyboardInput::X,
    KeyboardInput::Y,
    KeyboardInput::Z,
    KeyboardInput::TopRow1Exclamation,
    KeyboardInput::TopRow2At,
    KeyboardInput::TopRow3Hash,
    KeyboardInput::TopRow4Dollar,
    KeyboardInput::TopRow5Percent,
    KeyboardInput::TopRow6Caret,
    KeyboardInput::TopRow7Ampersand,
    KeyboardInput::TopRow8Asterisk,
    KeyboardInput::TopRow9OpenParens,
    KeyboardInput::TopRow0CloseParens,
    KeyboardInput::Enter,
    KeyboardInput::Backspace,
    KeyboardInput::Delete,
    KeyboardInput::Spacebar,
    KeyboardInput::DashUnderscore,
    KeyboardInput::EqualPlus,
    KeyboardInput::OpenBracketBrace,
    KeyboardInput::CloseBracketBrace,
    KeyboardInput::BackslashBar,
    KeyboardInput::NonUSHash,
    KeyboardInput::SemiColon,
    KeyboardInput::SingleDoubleQuote,
    KeyboardInput::BacktickTilde,
    KeyboardInput::CommaLessThan,
    KeyboardInput::PeriodGreaterThan,
    KeyboardInput::SlashQuestion,
    KeyboardInput::F1,
    KeyboardInput::F2,
    KeyboardInput::F3,
    KeyboardInput::F4,
    KeyboardInput::F5,
    KeyboardInput::F6,
    KeyboardInput::F7,
    KeyboardInput::F8,
    KeyboardInput::F9,
    KeyboardInput::F10,
    KeyboardInput::F11,
    KeyboardInput::F12,
    KeyboardInput::LeftControl,
    KeyboardInput::LeftShift,
    KeyboardInput::LeftAlt,
    KeyboardInput::LeftWindows,
    KeyboardInput::RightControl,
    KeyboardInput::RightShift,
    KeyboardInput::RightAlt,
    KeyboardInput::RightWindows,
    KeyboardInput::PrintScreen,
    KeyboardInput::Pause,
    KeyboardInput::Insert,
    KeyboardInput::Home,
    KeyboardInput::End,
    KeyboardInput::Power,
    KeyboardInput::Cut,
    KeyboardInput::Copy,
    KeyboardInput::Paste,
    KeyboardInput::Find,
    KeyboardInput::Mute,
    KeyboardInput::VolumeUp,
    KeyboardInput::VolumeDown,
];

#[derive(
    Format, Serialize, Deserialize, Debug, PartialEq, Default, Clone, Copy, EnumIter, IntoStaticStr,
)]
pub enum DPedalControl {
    /// By default all ComputerInputs in the output_sequence are held until the terminate_on condition is met.
    /// However, when this variant is included in the output_sequence, all elements after this one are blocked.
    /// Additionally, when the number of milliseconds have passed:
    /// * all previous elements are held
    /// * all future elements (until another AfterMillis*) are activated.
    AfterMillisHold(u16),

    /// By default all ComputerInputs in the output_sequence are held until the terminate_on condition is met.
    /// However, when this variant is included in the output_sequence, all elements after this one are blocked.
    /// Additionally, when the number of milliseconds have passed:
    /// * all previous elements are terminated
    /// * all future elements (until another AfterMillis*) are activated.
    AfterMillisRelease(u16),

    /// Restarts this output_sequence from the beginning.
    #[default]
    Restart,

    /// Change the profile to the configured index, any in progress output_sequences are terminated.
    SetProfile(u8),
}

impl DPedalControl {
    pub fn from_string(s: &str, value: &str) -> Option<Self> {
        match s {
            "AfterMillisHold" => Some(DPedalControl::AfterMillisHold(value.parse().ok()?)),
            "AfterMillisRelease" => Some(DPedalControl::AfterMillisRelease(value.parse().ok()?)),
            "Restart" => Some(DPedalControl::Restart),
            "SetProfile" => Some(DPedalControl::SetProfile(value.parse().ok()?)),
            _ => None,
        }
    }
}
