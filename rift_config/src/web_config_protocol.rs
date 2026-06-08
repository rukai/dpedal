use crate::{META_SERIALIZED_SIZE, PROFILE_SERIALIZED_SIZE};
use heapless::Vec;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, PartialEq)]
#[expect(clippy::large_enum_variant)]
pub enum Request {
    GetMeta,
    GetProfileCount,
    GetProfile(u8),
    SetMeta(Vec<u8, META_SERIALIZED_SIZE>),
    AddProfile(Vec<u8, PROFILE_SERIALIZED_SIZE>),
    ReloadConfig,
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
#[expect(clippy::large_enum_variant)]
pub enum Response {
    GetMeta(Result<Vec<u8, META_SERIALIZED_SIZE>, ()>),
    GetProfileCount(u8),
    GetProfile(Result<Vec<u8, PROFILE_SERIALIZED_SIZE>, ()>),
    SetMeta(Result<(), ()>),
    AddProfile(Result<(), ()>),
    ReloadConfig,
    ProtocolError,
}
