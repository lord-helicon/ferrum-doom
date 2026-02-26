# Architecture

This repository ships a browser-playable Doom source-port pipeline with a Rust-first WASM boundary and a thin TypeScript host.

## Repository Layout

- `engine/`
  - `engine_core` - Doom core integration layer (C source + Rust wrapper).
  - `engine_wad` - pure Rust WAD parser and lump indexing tests.
  - `engine_render` - framebuffer constants and render surface metadata.
  - `engine_sound` - Rust SFX mixer and ring-buffer PCM output.
  - `engine_music` - music event bus (MUS->MIDI registration/playback event flow).
  - `engine_platform` - shared config/input ABI types.
  - `engine_wasm` - exported WASM ABI and runtime orchestration.
- `platform_web/` - Vite + TypeScript browser host.
- `assets/` - placeholder only (no copyrighted WADs).
- `docs/` - architecture and validation docs.

## Engine Boundaries

### `engine_core`

- Compiles Doom core C modules from `engine_core/c_src/doomgeneric`.
- Exposes Rust wrappers around:
  - `doomgeneric_Create(argc, argv)`
  - `doomgeneric_Tick()`
  - `DG_ScreenBuffer`
- C platform callbacks are implemented in `engine_wasm` (`DG_GetKey`, `DG_GetMouse`, timing hooks, audio bridge callbacks).

### `engine_wad`

- Parses IWAD/PWAD headers and directory entries.
- Builds name index using latest-lump-wins semantics for PWAD overrides.
- Validates lump bounds and directory ranges.

### `engine_sound`

- Decodes Doom 8-bit unsigned PCM SFX into normalized samples.
- Mixes channels at fixed output sample rate.
- Maintains a lock-free style ringbuffer in WASM memory (`f32` stereo interleaved) for host pull.

### `engine_music`

- Maintains song registration cache and event queue (`Register`, `Play`, `Stop`, etc.).
- The C music backend (`i_websound.c`) converts MUS to MIDI using `mus2mid.c` + `memio.c` before registering events.
- Browser host synthesizes MIDI via WebAudio/Tone.

### `engine_platform`

- Defines shared ABI types (`EngineConfig`, `InputEvent`, tic constants).

### `engine_wasm`

- Stable exported ABI:
  - `init(config_ptr, config_len, wad_ptr, wad_len)`
  - `set_input(ptr, len)`
  - `run_tics(n)`
  - `framebuffer_ptr/len/width/height`
  - `audio_ringbuffer_ptr/len`
  - `audio_read_index_ptr/audio_write_index_ptr`
  - music polling ABI (`music_pop_event`, `music_event_*`)
- Owns runtime singleton: Doom core, input queues, sound mixer, music bus.

## JS/TS Host (`platform_web`)

- Uses Canvas2D with nearest-neighbor scaling.
- Uses `@bjorn3/browser_wasi_shim` to run WASI module in browser with a writable preopened directory.
- Input:
  - Keyboard -> Doom keycodes.
  - Mouse motion + buttons via pointer-lock relative events.
- Audio:
  - SFX pull from WASM ringbuffer through ScriptProcessorNode.
  - Music from MIDI events using Tone.js synth graph.
- Persistence:
  - Serializes preopened FS files to `localStorage` (`savegames`, configs, cached WAD blob).

## ABI Notes

- Framebuffer format: 32-bit packed pixels produced by Doom core memory layout.
- Audio format: interleaved stereo `f32` in range `[-1.0, 1.0]`.
- Engine tic rate: 35 Hz (classic Doom timing).
