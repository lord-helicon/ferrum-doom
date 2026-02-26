#![allow(clippy::missing_panics_doc)]

use engine_core::DoomCore;
use engine_music::{MusicBus, MusicEvent};
use engine_platform::{EngineConfig, InputEvent, InputEventKind};
use engine_render::{FRAME_HEIGHT, FRAME_PIXELS, FRAME_WIDTH};
use engine_sound::Mixer;
use engine_wad::Wad;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::{c_char, c_int};

const DEFAULT_SAMPLE_RATE: u32 = 44_100;

#[derive(Default)]
struct InputQueues {
    keys: VecDeque<(bool, u8)>,
    mouse: VecDeque<(i32, i32, i32)>,
}

struct Runtime {
    core: DoomCore,
    input: InputQueues,
    mixer: Mixer,
    music: MusicBus,
    last_music_event: MusicEvent,
}

impl Runtime {
    fn new(config: EngineConfig) -> anyhow::Result<Self> {
        let core = DoomCore::create(&config.args)?;
        Ok(Self {
            core,
            input: InputQueues::default(),
            mixer: Mixer::new(DEFAULT_SAMPLE_RATE, 4),
            music: MusicBus::default(),
            last_music_event: MusicEvent::none(),
        })
    }
}

thread_local! {
    static RUNTIME: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

fn with_runtime_mut<T>(f: impl FnOnce(&mut Runtime) -> T) -> Option<T> {
    RUNTIME.with(|slot| {
        let mut borrow = slot.borrow_mut();
        let runtime = borrow.as_mut()?;
        Some(f(runtime))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buf = vec![0u8; len].into_boxed_slice();
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[unsafe(no_mangle)]
pub extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }

    unsafe {
        let _ = Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn init(
    config_ptr: *const u8,
    config_len: usize,
    wad_ptr: *const u8,
    wad_len: usize,
) -> i32 {
    if config_ptr.is_null() || wad_ptr.is_null() || config_len == 0 || wad_len == 0 {
        return -1;
    }

    let config_bytes = unsafe { std::slice::from_raw_parts(config_ptr, config_len) };
    let wad_bytes = unsafe { std::slice::from_raw_parts(wad_ptr, wad_len) };

    let config: EngineConfig = match serde_json::from_slice(config_bytes) {
        Ok(cfg) => cfg,
        Err(_) => return -2,
    };

    if Wad::parse(wad_bytes.to_vec()).is_err() {
        return -3;
    }

    if std::fs::write(&config.iwad_virtual_path, wad_bytes).is_err() {
        return -4;
    }

    match Runtime::new(config) {
        Ok(rt) => {
            RUNTIME.with(|slot| {
                *slot.borrow_mut() = Some(rt);
            });
            0
        }
        Err(_) => -5,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn set_input(events_ptr: *const InputEvent, events_len: usize) {
    if events_ptr.is_null() || events_len == 0 {
        return;
    }

    let events = unsafe { std::slice::from_raw_parts(events_ptr, events_len) };
    let _ = with_runtime_mut(|rt| {
        for event in events {
            match event.kind {
                x if x == InputEventKind::Key as u8 => {
                    rt.input.keys.push_back((event.a != 0, event.b as u8));
                }
                x if x == InputEventKind::Mouse as u8 => {
                    rt.input.mouse.push_back((event.a, event.b, event.c));
                }
                _ => {}
            }
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn queue_key_event(pressed: i32, key: i32) {
    let _ = with_runtime_mut(|rt| rt.input.keys.push_back((pressed != 0, key as u8)));
}

#[unsafe(no_mangle)]
pub extern "C" fn queue_mouse_event(buttons: i32, dx: i32, dy: i32) {
    let _ = with_runtime_mut(|rt| rt.input.mouse.push_back((buttons, dx, dy)));
}

#[unsafe(no_mangle)]
pub extern "C" fn run_tics(count: u32) {
    if count == 0 {
        return;
    }

    let _ = with_runtime_mut(|rt| {
        for _ in 0..count {
            while let Some((pressed, key)) = rt.input.keys.pop_front() {
                rt.core.key_event(pressed, key);
            }
            while let Some((_buttons, dx, dy)) = rt.input.mouse.pop_front() {
                rt.core.mouse_event(dx, dy);
            }
            rt.core.tick();
        }
        rt.mixer.mix_tics(count);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn framebuffer_ptr() -> *const u32 {
    with_runtime_mut(|rt| rt.core.framebuffer_ptr()).unwrap_or(std::ptr::null())
}

#[unsafe(no_mangle)]
pub extern "C" fn framebuffer_len() -> usize {
    FRAME_PIXELS
}

#[unsafe(no_mangle)]
pub extern "C" fn framebuffer_width() -> u32 {
    FRAME_WIDTH as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn framebuffer_height() -> u32 {
    FRAME_HEIGHT as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn audio_ringbuffer_ptr() -> *const f32 {
    with_runtime_mut(|rt| rt.mixer.ring_info().ptr).unwrap_or(std::ptr::null())
}

#[unsafe(no_mangle)]
pub extern "C" fn audio_ringbuffer_len() -> usize {
    with_runtime_mut(|rt| rt.mixer.ring_info().len).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn audio_read_index_ptr() -> *mut u32 {
    with_runtime_mut(|rt| rt.mixer.read_index_mut_ptr()).unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "C" fn audio_write_index_ptr() -> *const u32 {
    with_runtime_mut(|rt| rt.mixer.write_index_ptr()).unwrap_or(std::ptr::null())
}

#[unsafe(no_mangle)]
pub extern "C" fn set_sfx_master_volume(volume: f32) {
    let _ = with_runtime_mut(|rt| rt.mixer.set_master_volume(volume));
}

#[unsafe(no_mangle)]
pub extern "C" fn music_pop_event() -> u32 {
    with_runtime_mut(|rt| {
        if let Some(ev) = rt.music.pop_event() {
            let kind = ev.kind as u32;
            rt.last_music_event = ev;
            kind
        } else {
            0
        }
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn music_event_song_id() -> u32 {
    with_runtime_mut(|rt| rt.last_music_event.song_id).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn music_event_value() -> i32 {
    with_runtime_mut(|rt| rt.last_music_event.value).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn music_event_data_ptr() -> *const u8 {
    with_runtime_mut(|rt| rt.last_music_event.payload.as_ptr()).unwrap_or(std::ptr::null())
}

#[unsafe(no_mangle)]
pub extern "C" fn music_event_data_len() -> usize {
    with_runtime_mut(|rt| rt.last_music_event.payload.len()).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn current_state_crc() -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    let _ = with_runtime_mut(|rt| {
        for px in rt.core.framebuffer_words() {
            hasher.update(&px.to_le_bytes());
        }
    });
    hasher.finalize()
}

// Legacy ABI symbols retained for host compatibility.
#[unsafe(no_mangle)]
pub extern "C" fn DG_Init() {}

#[unsafe(no_mangle)]
pub extern "C" fn DG_DrawFrame() {}

#[unsafe(no_mangle)]
pub extern "C" fn DG_SleepMs(_ms: u32) {}

#[unsafe(no_mangle)]
pub extern "C" fn DG_GetTicksMs() -> u32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_GetKey(_pressed: *mut c_int, _key: *mut u8) -> c_int {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_GetMouse(_buttons: *mut c_int, _xrel: *mut c_int, _yrel: *mut c_int) -> c_int {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_SetWindowTitle(_title: *const c_char) {}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustSfxStart(
    _channel: c_int,
    _data_ptr: *const u8,
    _data_len: c_int,
    _samplerate: c_int,
    _volume: c_int,
    _sep: c_int,
) {
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustSfxStop(_channel: c_int) {}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustSfxUpdateParams(_channel: c_int, _volume: c_int, _sep: c_int) {}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustSfxIsPlaying(_channel: c_int) -> c_int {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustMusicRegister(_song_id: c_int, _data_ptr: *const u8, _data_len: c_int) {}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustMusicUnregister(_song_id: c_int) {}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustMusicPlay(_song_id: c_int, _looping: c_int) {}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustMusicStop() {}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustMusicPause(_paused: c_int) {}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustMusicVolume(_volume: c_int) {}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustMusicIsPlaying() -> c_int {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_buf(bytes: &[u8]) -> (usize, usize) {
        let ptr = alloc(bytes.len()) as usize;
        let dst = ptr as *mut u8;
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
        }
        (ptr, bytes.len())
    }

    #[test]
    fn rejects_invalid_wad() {
        let config = serde_json::json!({
            "iwad_virtual_path": "DOOM.WAD",
            "args": ["doom", "-iwad", "DOOM.WAD"]
        });
        let config_bytes = serde_json::to_vec(&config).expect("config json");
        let bad_wad = [1u8, 2u8, 3u8, 4u8];

        let (cfg_ptr, cfg_len) = write_buf(&config_bytes);
        let (wad_ptr, wad_len) = write_buf(&bad_wad);

        let rc = init(cfg_ptr as *const u8, cfg_len, wad_ptr as *const u8, wad_len);

        dealloc(cfg_ptr as *mut u8, cfg_len);
        dealloc(wad_ptr as *mut u8, wad_len);

        assert_eq!(rc, -3);
    }

    #[test]
    fn deterministic_crc_with_real_iwad_when_available() {
        let Ok(path) = std::env::var("DOOM_IWAD") else {
            return;
        };

        let wad = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => return,
        };

        let config = serde_json::json!({
            "iwad_virtual_path": "DOOM_TEST.WAD",
            "args": ["doom", "-iwad", "DOOM_TEST.WAD"]
        });
        let config_bytes = serde_json::to_vec(&config).expect("config json");

        let run_once = |wad: &[u8], config_bytes: &[u8]| -> Option<u32> {
            let (cfg_ptr, cfg_len) = write_buf(config_bytes);
            let (wad_ptr, wad_len) = write_buf(wad);
            let rc = init(cfg_ptr as *const u8, cfg_len, wad_ptr as *const u8, wad_len);
            dealloc(cfg_ptr as *mut u8, cfg_len);
            dealloc(wad_ptr as *mut u8, wad_len);
            if rc != 0 {
                return None;
            }
            run_tics(210);
            Some(current_state_crc())
        };

        let first = run_once(&wad, &config_bytes);
        let second = run_once(&wad, &config_bytes);
        assert_eq!(first, second);
    }
}
