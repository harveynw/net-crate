use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(TS, Serialize)]
#[ts(export)]
pub enum ServerMessage {
    Update(HashMap<u32, (f32, f32, f32)>),
    PlayerJoined(u32),
    PlayerLeft(u32)
}

#[derive(TS, Deserialize)]
#[ts(export)]
pub enum ClientMessage {
    Move(f32, f32, f32),
}

pub fn serialize(message: ServerMessage) -> Vec<u8> {
    serde_json::to_vec(&message).unwrap()
}

pub fn deserialize(message: Vec<u8>) -> ClientMessage {
    serde_json::from_str(&String::from_utf8(message).unwrap()).unwrap()
}