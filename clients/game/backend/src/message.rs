use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(TS, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    position: [f32; 3],
    up: [f32; 3],
    rotate: [f32; 4],
    movement_state: String
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            rotate: [0.0, 0.0, 0.0, 1.0],
            movement_state: String::from("Idle")
        }
    }
}

#[derive(TS, Serialize)]
#[ts(export)]
pub enum ServerMessage {
    Update(HashMap<u32, PlayerState>),
    PlayerJoined(u32),
    PlayerLeft(u32)
}

#[derive(TS, Deserialize)]
#[ts(export)]
pub enum ClientMessage {
    Update(PlayerState),
}

pub fn serialize(message: ServerMessage) -> Vec<u8> {
    serde_json::to_vec(&message).unwrap()
}

pub fn deserialize(message: Vec<u8>) -> ClientMessage {
    serde_json::from_str(&String::from_utf8(message).unwrap()).unwrap()
}