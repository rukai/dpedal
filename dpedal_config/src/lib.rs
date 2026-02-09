#![no_std]

pub mod web_config_protocol;

// Memory layout
pub const RP2040_FLASH_OFFSET: usize = 0x10000000;
pub const PICO_FLASH_SIZE: usize = 1024 * 1024 * 2; // 2 MiB

pub const FIRMWARE_OFFSET: usize = 0;
pub const FIRMWARE_SIZE: usize = 1024 * 2000; // 2000 KiB
pub const CONFIG_OFFSET: usize = 1024 * 2000;
pub const CONFIG_SIZE: usize = 1024 * 16; // 16 KiB // TODO: give more space to config, currently increasing this causes problems, see dpedal_firmware/src/config.rs

use arrayvec::{ArrayString, ArrayVec};
use defmt::Format;
use rkyv::{Archive, Deserialize, Serialize};
use strum::{EnumIter, EnumString, IntoEnumIterator};

const fn assert_config_fits_in_flash() {
    // TODO: This isnt actually accurate, since the data will be serialized into rkyv format first.
    //       Is there a way to calculate the maximum possible size in rkyv format?
    assert!(core::mem::size_of::<Config>() <= CONFIG_SIZE);
}

const fn assert_config_size_fits_into_writable_flash_blocks() {
    // Flash can only be written in blocks of 4096 bytes.
    assert!(CONFIG_OFFSET.is_multiple_of(4096));
    assert!(CONFIG_SIZE.is_multiple_of(4096));
}

const _: () = assert_config_fits_in_flash();
const _: () = assert_config_size_fits_into_writable_flash_blocks();

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq, Clone)]
#[rkyv(derive(Debug))]
pub struct Config {
    pub version: u32,
    pub nickname: ArrayString<50>,
    pub device: Device,
    pub color: u32,
    pub profiles: ArrayVec<Profile, 2>,
    pub pin_remappings: ArrayVec<PinRemapping, 6>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: Default::default(),
            nickname: ArrayString::from("my DPedal").unwrap(),
            device: Default::default(),
            color: 0x1790e3,
            profiles: ArrayVec::from_iter([Profile {
                mappings: ArrayVec::from_iter([
                    Mapping {
                        input: ArrayVec::from_iter([DpedalInput::DpadLeft]),
                        output: ArrayVec::from_iter([ComputerInput::Mouse(
                            MouseInput::ScrollLeft(10),
                        )]),
                    },
                    Mapping {
                        input: ArrayVec::from_iter([DpedalInput::DpadRight]),
                        output: ArrayVec::from_iter([ComputerInput::Mouse(
                            MouseInput::ScrollRight(10),
                        )]),
                    },
                    Mapping {
                        input: ArrayVec::from_iter([DpedalInput::DpadUp]),
                        output: ArrayVec::from_iter([ComputerInput::Mouse(MouseInput::ScrollUp(
                            10,
                        ))]),
                    },
                    Mapping {
                        input: ArrayVec::from_iter([DpedalInput::DpadDown]),
                        output: ArrayVec::from_iter([ComputerInput::Mouse(
                            MouseInput::ScrollDown(10),
                        )]),
                    },
                    Mapping {
                        input: ArrayVec::from_iter([DpedalInput::ButtonLeft]),
                        output: ArrayVec::from_iter([ComputerInput::Keyboard(
                            KeyboardInput::PageUp,
                        )]),
                    },
                    Mapping {
                        input: ArrayVec::from_iter([DpedalInput::ButtonRight]),
                        output: ArrayVec::from_iter([ComputerInput::Keyboard(
                            KeyboardInput::PageDown,
                        )]),
                    },
                ]),
            }]),
            pin_remappings: Default::default(),
        }
    }
}

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq, Default, Clone)]
#[rkyv(derive(Debug))]
pub enum Device {
    #[default]
    Dpedal,
}

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq, Default, Clone)]
#[rkyv(derive(Debug))]
pub struct PinRemapping {
    pub input: DpedalInput,
    // TODO: make u8
    pub pin: u32,
}

pub const MAX_MAPPINGS: usize = 20;
#[derive(Archive, Deserialize, Serialize, Debug, PartialEq, Default, Clone)]
#[rkyv(derive(Debug))]
pub struct Profile {
    pub mappings: ArrayVec<Mapping, MAX_MAPPINGS>,
}

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq, Default, Clone)]
#[rkyv(derive(Debug))]
pub struct Mapping {
    pub input: ArrayVec<DpedalInput, 4>,
    pub output: ArrayVec<ComputerInput, 20>,
}

#[derive(Format, Archive, Deserialize, Serialize, Debug, PartialEq, Default, Clone, Copy)]
#[rkyv(derive(Debug))]
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

#[derive(Format, Archive, Deserialize, Serialize, Debug, PartialEq, Default, Clone, Copy)]
#[rkyv(derive(Debug))]
pub enum ComputerInput {
    #[default]
    None, // TODO: remove?
    Mouse(MouseInput),
    Keyboard(KeyboardInput),
    Control(DPedalControl),
}

#[derive(
    Format, Archive, Deserialize, Serialize, Debug, PartialEq, Default, Clone, Copy, EnumIter,
)]
#[rkyv(derive(Debug))]
pub enum MouseInput {
    ScrollUp(i16),
    ScrollDown(i16),
    ScrollRight(i16),
    ScrollLeft(i16),
    MoveUp(i16),
    MoveDown(i16),
    MoveRight(i16),
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
    Format,
    Archive,
    Deserialize,
    Serialize,
    Debug,
    PartialEq,
    Default,
    Clone,
    Copy,
    EnumIter,
    EnumString,
)]
#[rkyv(derive(Debug))]
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
    /// Keypad 00
    Keypad00 = 0xB0,
    /// Keypad 000
    Keypad000 = 0xB1,
    /// Thousands Separator (Footnote 31)
    ThousandsSeparator = 0xB2,
    /// Decimal Separator (Footnote 31)
    DecimalSeparator = 0xB3,
    /// Currency Unit (Footnote 32)
    CurrencyUnit = 0xB4,
    /// Currency Sub-unit (Footnote 32)
    CurrencySubunit = 0xB5,
    /// Keypad (
    KeypadOpenParens = 0xB6,
    /// Keypad )
    KeypadCloseParens = 0xB7,
    /// Keypad {
    KeypadOpenBrace = 0xB8,
    /// Keypad }
    KeypadCloseBrace = 0xB9,
    /// Keypad Tab
    KeypadTab = 0xBA,
    /// Keypad Backspace
    KeypadBackspace = 0xBB,
    /// Keypad A
    KeypadA = 0xBC,
    /// Keypad B
    KeypadB = 0xBD,
    /// Keypad C
    KeypadC = 0xBE,
    /// Keypad D
    KeypadD = 0xBF,
    /// Keypad E
    KeypadE = 0xC0,
    /// Keypad F
    KeypadF = 0xC1,
    /// Keypad XOR
    KeypadBitwiseXor = 0xC2,
    /// Keypad ^
    KeypadLogicalXor = 0xC3,
    /// Keypad %
    KeypadModulo = 0xC4,
    /// Keypad <
    KeypadLeftShift = 0xC5,
    /// Keypad >
    KeypadRightShift = 0xC6,
    /// Keypad &
    KeypadBitwiseAnd = 0xC7,
    /// Keypad &&
    KeypadLogicalAnd = 0xC8,
    /// Keypad |
    KeypadBitwiseOr = 0xC9,
    /// Keypad ||
    KeypadLogicalOr = 0xCA,
    /// Keypad :
    KeypadColon = 0xCB,
    /// Keypad #
    KeypadHash = 0xCC,
    /// Keypad Space
    KeypadSpace = 0xCD,
    /// Keypad @
    KeypadAt = 0xCE,
    /// Keypad !
    KeypadExclamation = 0xCF,
    /// Keypad Memory Store
    KeypadMemoryStore = 0xD0,
    /// Keypad Memory Recall
    KeypadMemoryRecall = 0xD1,
    /// Keypad Memory Clear
    KeypadMemoryClear = 0xD2,
    /// Keypad Memory Add
    KeypadMemoryAdd = 0xD3,
    /// Keypad Memory Subtract
    KeypadMemorySubtract = 0xD4,
    /// Keypad Memory Multiply
    KeypadMemoryMultiply = 0xD5,
    /// Keypad Memory Divice
    KeypadMemoryDivide = 0xD6,
    /// Keypad +/-
    KeypadPositiveNegative = 0xD7,
    /// Keypad Clear
    KeypadClear = 0xD8,
    /// Keypad Clear Entry
    KeypadClearEntry = 0xD9,
    /// Keypad Binary
    KeypadBinary = 0xDA,
    /// Keypad Octal
    KeypadOctal = 0xDB,
    /// Keypad Decimal
    KeypadDecimal = 0xDC,
    /// Keypad Hexadecimal
    KeypadHexadecimal = 0xDD,
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
    Format, Archive, Deserialize, Serialize, Debug, PartialEq, Default, Clone, Copy, EnumIter,
)]
#[rkyv(derive(Debug))]
pub enum DPedalControl {
    #[default]
    DoNothing,
    // ReleaseAndSleep(u16)
    // HoldAndSleep(u16)
    // SetProfile(u8)
}

impl DPedalControl {
    pub fn from_string(s: &str) -> Option<Self> {
        match s {
            "DoNothing" => Some(DPedalControl::DoNothing),
            _ => None,
        }
    }
}
