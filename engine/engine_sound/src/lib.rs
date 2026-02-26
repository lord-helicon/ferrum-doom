#![deny(unsafe_code)]

use engine_platform::TIC_RATE_HZ;

const CHANNEL_COUNT: usize = 16;

#[derive(Clone, Default)]
struct Channel {
    data: Vec<f32>,
    pos: f32,
    step: f32,
    left: f32,
    right: f32,
    active: bool,
}

impl Channel {
    fn reset(&mut self) {
        self.data.clear();
        self.pos = 0.0;
        self.step = 1.0;
        self.left = 0.0;
        self.right = 0.0;
        self.active = false;
    }
}

#[derive(Debug)]
pub struct RingBufferInfo {
    pub ptr: *const f32,
    pub len: usize,
    pub read_index_ptr: *const u32,
    pub write_index_ptr: *const u32,
}

pub struct Mixer {
    sample_rate: u32,
    channels: [Channel; CHANNEL_COUNT],
    ring: Box<[f32]>,
    read_index: u32,
    write_index: u32,
    frac_accum: u64,
    master_volume: f32,
}

impl Mixer {
    pub fn new(sample_rate: u32, seconds: u32) -> Self {
        let frames = (sample_rate.saturating_mul(seconds) as usize).max(2048);
        Self {
            sample_rate,
            channels: std::array::from_fn(|_| Channel::default()),
            ring: vec![0.0; frames * 2].into_boxed_slice(),
            read_index: 0,
            write_index: 0,
            frac_accum: 0,
            master_volume: 1.0,
        }
    }

    pub fn ring_info(&self) -> RingBufferInfo {
        RingBufferInfo {
            ptr: self.ring.as_ptr(),
            len: self.ring.len(),
            read_index_ptr: &self.read_index,
            write_index_ptr: &self.write_index,
        }
    }

    pub fn read_index_mut_ptr(&mut self) -> *mut u32 {
        &mut self.read_index
    }

    pub fn write_index_ptr(&self) -> *const u32 {
        &self.write_index
    }

    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.clamp(0.0, 1.0);
    }

    pub fn start_sound(
        &mut self,
        channel: usize,
        raw_unsigned_pcm: &[u8],
        src_rate: u32,
        volume: i32,
        sep: i32,
    ) {
        if channel >= CHANNEL_COUNT || raw_unsigned_pcm.is_empty() || src_rate == 0 {
            return;
        }

        let mut converted = Vec::with_capacity(raw_unsigned_pcm.len());
        for s in raw_unsigned_pcm {
            converted.push((*s as f32 - 128.0) / 128.0);
        }

        let (left, right) = pan_from_sep(volume, sep);
        let ch = &mut self.channels[channel];
        ch.data = converted;
        ch.pos = 0.0;
        ch.step = src_rate as f32 / self.sample_rate as f32;
        ch.left = left;
        ch.right = right;
        ch.active = true;
    }

    pub fn stop_sound(&mut self, channel: usize) {
        if channel < CHANNEL_COUNT {
            self.channels[channel].reset();
        }
    }

    pub fn update_sound_params(&mut self, channel: usize, volume: i32, sep: i32) {
        if channel >= CHANNEL_COUNT {
            return;
        }
        let (left, right) = pan_from_sep(volume, sep);
        let ch = &mut self.channels[channel];
        if ch.active {
            ch.left = left;
            ch.right = right;
        }
    }

    pub fn sound_is_playing(&self, channel: usize) -> bool {
        channel < CHANNEL_COUNT && self.channels[channel].active
    }

    pub fn mix_tics(&mut self, tic_count: u32) {
        if tic_count == 0 {
            return;
        }

        self.frac_accum = self
            .frac_accum
            .saturating_add(u64::from(self.sample_rate).saturating_mul(u64::from(tic_count)));
        let frames = (self.frac_accum / u64::from(TIC_RATE_HZ)) as usize;
        self.frac_accum %= u64::from(TIC_RATE_HZ);

        if frames == 0 {
            return;
        }

        for _ in 0..frames {
            let mut l = 0.0f32;
            let mut r = 0.0f32;

            for ch in &mut self.channels {
                if !ch.active {
                    continue;
                }
                let idx = ch.pos as usize;
                if idx >= ch.data.len() {
                    ch.reset();
                    continue;
                }
                let sample = ch.data[idx];
                l += sample * ch.left;
                r += sample * ch.right;
                ch.pos += ch.step;
                if ch.pos as usize >= ch.data.len() {
                    ch.reset();
                }
            }

            self.push_stereo(
                (l * self.master_volume).clamp(-1.0, 1.0),
                (r * self.master_volume).clamp(-1.0, 1.0),
            );
        }
    }

    fn push_stereo(&mut self, left: f32, right: f32) {
        let cap_frames = (self.ring.len() / 2) as u32;
        if cap_frames < 2 {
            return;
        }

        let next_write = (self.write_index + 1) % cap_frames;
        if next_write == self.read_index {
            self.read_index = (self.read_index + 1) % cap_frames;
        }

        let base = (self.write_index as usize) * 2;
        self.ring[base] = left;
        self.ring[base + 1] = right;
        self.write_index = next_write;
    }
}

fn pan_from_sep(volume: i32, sep: i32) -> (f32, f32) {
    let vol = (volume.clamp(0, 127) as f32) / 127.0;
    let sep = sep.clamp(0, 254) as f32;
    let left = ((254.0 - sep) / 254.0) * vol;
    let right = (sep / 254.0) * vol;
    (left, right)
}
