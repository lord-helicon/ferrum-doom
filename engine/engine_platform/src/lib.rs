#![deny(unsafe_code)]

use serde::{Deserialize, Serialize};

pub const TIC_RATE_HZ: u32 = 35;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum InputEventKind {
    Key = 1,
    Mouse = 2,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(C)]
pub struct InputEvent {
    pub kind: u8,
    pub a: i32,
    pub b: i32,
    pub c: i32,
}

impl InputEvent {
    pub fn key(pressed: bool, key: u8) -> Self {
        Self {
            kind: InputEventKind::Key as u8,
            a: i32::from(pressed),
            b: i32::from(key),
            c: 0,
        }
    }

    pub fn mouse(button_mask: i32, dx: i32, dy: i32) -> Self {
        Self {
            kind: InputEventKind::Mouse as u8,
            a: button_mask,
            b: dx,
            c: dy,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EngineConfig {
    pub iwad_virtual_path: String,
    pub args: Vec<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            iwad_virtual_path: "DOOM.WAD".to_string(),
            args: vec![
                "doom".to_string(),
                "-iwad".to_string(),
                "DOOM.WAD".to_string(),
            ],
        }
    }
}
