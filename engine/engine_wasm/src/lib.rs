#![allow(clippy::missing_panics_doc)]

use engine_core::DoomCore;
use engine_music::{MusicBus, MusicEvent};
use engine_platform::{EngineConfig, InputEvent, InputEventKind};
use engine_render::{FRAME_HEIGHT, FRAME_PIXELS, FRAME_WIDTH};
use engine_sound::Mixer;
use engine_wad::Wad;
use std::collections::VecDeque;
use std::ffi::{c_char, c_int};
use std::sync::{Mutex, OnceLock};

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

static RUNTIME: OnceLock<Mutex<Option<Runtime>>> = OnceLock::new();

fn runtime_lock() -> &'static Mutex<Option<Runtime>> {
    RUNTIME.get_or_init(|| Mutex::new(None))
}

fn with_runtime_mut<T>(f: impl FnOnce(&mut Runtime) -> T) -> Option<T> {
    let lock = runtime_lock();
    let mut guard = lock.lock().ok()?;
    let runtime = guard.as_mut()?;
    Some(f(runtime))
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
    // SAFETY: pointer must have been returned from alloc() with same len.
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
            let lock = runtime_lock();
            if let Ok(mut guard) = lock.lock() {
                *guard = Some(rt);
                0
            } else {
                -6
            }
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
    let _ = with_runtime_mut(|rt| {
        rt.input.keys.push_back((pressed != 0, key as u8));
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn queue_mouse_event(buttons: i32, dx: i32, dy: i32) {
    let _ = with_runtime_mut(|rt| {
        rt.input.mouse.push_back((buttons, dx, dy));
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn run_tics(count: u32) {
    if count == 0 {
        return;
    }

    let _ = with_runtime_mut(|rt| {
        for _ in 0..count {
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
        let fb_ptr = rt.core.framebuffer_ptr();
        if !fb_ptr.is_null() {
            let fb = unsafe { std::slice::from_raw_parts(fb_ptr as *const u8, FRAME_PIXELS * 4) };
            hasher.update(fb);
        }
    });
    hasher.finalize()
}

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
pub extern "C" fn DG_GetKey(pressed: *mut c_int, key: *mut u8) -> c_int {
    if pressed.is_null() || key.is_null() {
        return 0;
    }

    with_runtime_mut(|rt| {
        if let Some((is_pressed, keycode)) = rt.input.keys.pop_front() {
            unsafe {
                *pressed = i32::from(is_pressed);
                *key = keycode;
            }
            1
        } else {
            0
        }
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_GetMouse(buttons: *mut c_int, xrel: *mut c_int, yrel: *mut c_int) -> c_int {
    if buttons.is_null() || xrel.is_null() || yrel.is_null() {
        return 0;
    }

    with_runtime_mut(|rt| {
        if let Some((b, x, y)) = rt.input.mouse.pop_front() {
            unsafe {
                *buttons = b;
                *xrel = x;
                *yrel = y;
            }
            1
        } else {
            0
        }
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_SetWindowTitle(_title: *const c_char) {}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustSfxStart(
    channel: c_int,
    data_ptr: *const u8,
    data_len: c_int,
    samplerate: c_int,
    volume: c_int,
    sep: c_int,
) {
    if data_ptr.is_null() || data_len <= 0 || samplerate <= 0 || channel < 0 {
        return;
    }

    let bytes = unsafe { std::slice::from_raw_parts(data_ptr, data_len as usize) };
    let _ = with_runtime_mut(|rt| {
        rt.mixer
            .start_sound(channel as usize, bytes, samplerate as u32, volume, sep);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustSfxStop(channel: c_int) {
    if channel < 0 {
        return;
    }
    let _ = with_runtime_mut(|rt| rt.mixer.stop_sound(channel as usize));
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustSfxUpdateParams(channel: c_int, volume: c_int, sep: c_int) {
    if channel < 0 {
        return;
    }
    let _ = with_runtime_mut(|rt| rt.mixer.update_sound_params(channel as usize, volume, sep));
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustSfxIsPlaying(channel: c_int) -> c_int {
    if channel < 0 {
        return 0;
    }
    with_runtime_mut(|rt| i32::from(rt.mixer.sound_is_playing(channel as usize))).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustMusicRegister(song_id: c_int, data_ptr: *const u8, data_len: c_int) {
    if song_id < 0 || data_ptr.is_null() || data_len <= 0 {
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data_ptr, data_len as usize) };
    let _ = with_runtime_mut(|rt| rt.music.register_song(song_id as u32, bytes.to_vec()));
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustMusicUnregister(song_id: c_int) {
    if song_id < 0 {
        return;
    }
    let _ = with_runtime_mut(|rt| rt.music.unregister_song(song_id as u32));
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustMusicPlay(song_id: c_int, looping: c_int) {
    if song_id < 0 {
        return;
    }
    let _ = with_runtime_mut(|rt| rt.music.play_song(song_id as u32, looping != 0));
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustMusicStop() {
    let _ = with_runtime_mut(|rt| rt.music.stop_song());
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustMusicPause(paused: c_int) {
    let _ = with_runtime_mut(|rt| rt.music.set_pause(paused != 0));
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustMusicVolume(volume: c_int) {
    let _ = with_runtime_mut(|rt| rt.music.set_volume(volume));
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_RustMusicIsPlaying() -> c_int {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_buf(bytes: &[u8]) -> (usize, usize) {
        let ptr = alloc(bytes.len()) as usize;
        let dst = ptr as *mut u8;
        // SAFETY: allocation returned by alloc is valid for bytes.len().
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
