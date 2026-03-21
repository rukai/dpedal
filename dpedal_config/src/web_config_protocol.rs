use crate::CONFIG_SIZE;
use heapless::Vec;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, PartialEq)]
#[expect(clippy::large_enum_variant)]
pub enum Request {
    GetConfig,
    SetConfig(Vec<u8, CONFIG_SIZE>),
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
#[expect(clippy::large_enum_variant)]
pub enum Response {
    GetConfig(Result<Vec<u8, CONFIG_SIZE>, ()>),
    SetConfig,
    ProtocolError,
}
