#![deny(unsafe_code)]

use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MusicEventKind {
    None = 0,
    Register = 1,
    Unregister = 2,
    Play = 3,
    Stop = 4,
    Pause = 5,
    Volume = 6,
}

#[derive(Debug, Clone)]
pub struct MusicEvent {
    pub kind: MusicEventKind,
    pub song_id: u32,
    pub value: i32,
    pub payload: Vec<u8>,
}

impl MusicEvent {
    pub fn none() -> Self {
        Self {
            kind: MusicEventKind::None,
            song_id: 0,
            value: 0,
            payload: Vec::new(),
        }
    }
}

#[derive(Default)]
pub struct MusicBus {
    songs: HashMap<u32, Vec<u8>>,
    queue: VecDeque<MusicEvent>,
    current: Option<u32>,
}

impl MusicBus {
    pub fn register_song(&mut self, song_id: u32, bytes: Vec<u8>) {
        self.songs.insert(song_id, bytes.clone());
        self.queue.push_back(MusicEvent {
            kind: MusicEventKind::Register,
            song_id,
            value: 0,
            payload: bytes,
        });
    }

    pub fn unregister_song(&mut self, song_id: u32) {
        self.songs.remove(&song_id);
        if self.current == Some(song_id) {
            self.current = None;
        }
        self.queue.push_back(MusicEvent {
            kind: MusicEventKind::Unregister,
            song_id,
            value: 0,
            payload: Vec::new(),
        });
    }

    pub fn play_song(&mut self, song_id: u32, looping: bool) {
        self.current = Some(song_id);
        self.queue.push_back(MusicEvent {
            kind: MusicEventKind::Play,
            song_id,
            value: i32::from(looping),
            payload: Vec::new(),
        });
    }

    pub fn stop_song(&mut self) {
        self.current = None;
        self.queue.push_back(MusicEvent {
            kind: MusicEventKind::Stop,
            song_id: 0,
            value: 0,
            payload: Vec::new(),
        });
    }

    pub fn set_pause(&mut self, paused: bool) {
        self.queue.push_back(MusicEvent {
            kind: MusicEventKind::Pause,
            song_id: 0,
            value: i32::from(paused),
            payload: Vec::new(),
        });
    }

    pub fn set_volume(&mut self, volume: i32) {
        self.queue.push_back(MusicEvent {
            kind: MusicEventKind::Volume,
            song_id: 0,
            value: volume,
            payload: Vec::new(),
        });
    }

    pub fn pop_event(&mut self) -> Option<MusicEvent> {
        self.queue.pop_front()
    }

    pub fn song_bytes(&self, song_id: u32) -> Option<&[u8]> {
        self.songs.get(&song_id).map(Vec::as_slice)
    }
}
