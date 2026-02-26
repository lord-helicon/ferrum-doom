import {
  ConsoleStdout,
  Directory,
  File,
  OpenFile,
  PreopenDirectory,
  WASI,
  wasi,
} from "@bjorn3/browser_wasi_shim";
import { Midi } from "@tonejs/midi";
import * as Tone from "tone";

type EngineExports = {
  memory: WebAssembly.Memory;
  alloc(len: number): number;
  dealloc(ptr: number, len: number): void;
  init(configPtr: number, configLen: number, wadPtr: number, wadLen: number): number;
  queue_key_event(pressed: number, key: number): void;
  queue_mouse_event(buttons: number, dx: number, dy: number): void;
  run_tics(count: number): void;
  framebuffer_ptr(): number;
  framebuffer_len(): number;
  framebuffer_width(): number;
  framebuffer_height(): number;
  audio_ringbuffer_ptr(): number;
  audio_ringbuffer_len(): number;
  audio_read_index_ptr(): number;
  audio_write_index_ptr(): number;
  set_sfx_master_volume(v: number): void;
  music_pop_event(): number;
  music_event_song_id(): number;
  music_event_value(): number;
  music_event_data_ptr(): number;
  music_event_data_len(): number;
};

const TIC_MS = 1000 / 35;
const STORAGE_KEY = "webdoom_fs_v1";
const WAD_STORAGE_KEY = "webdoom_wad_cache_v1";

const KEY = {
  LEFT: 0xac,
  RIGHT: 0xae,
  UP: 0xad,
  DOWN: 0xaf,
  STRAFE_L: 0xa0,
  STRAFE_R: 0xa1,
  USE: 0xa2,
  FIRE: 0xa3,
  ESC: 27,
  ENTER: 13,
  TAB: 9,
  RSHIFT: 0xb6,
  RALT: 0xb8,
  BACKSPACE: 0x7f,
  F1: 0xbb,
  F2: 0xbc,
  F3: 0xbd,
  F4: 0xbe,
  F5: 0xbf,
  F6: 0xc0,
  F7: 0xc1,
  F8: 0xc2,
  F9: 0xc3,
  F10: 0xc4,
  F11: 0xd7,
};

const keyMap = new Map<string, number>([
  ["ArrowLeft", KEY.LEFT],
  ["ArrowRight", KEY.RIGHT],
  ["ArrowUp", KEY.UP],
  ["ArrowDown", KEY.DOWN],
  ["ShiftLeft", KEY.RSHIFT],
  ["ShiftRight", KEY.RSHIFT],
  ["AltLeft", KEY.RALT],
  ["AltRight", KEY.RALT],
  ["ControlLeft", KEY.FIRE],
  ["ControlRight", KEY.FIRE],
  ["Space", KEY.USE],
  ["Escape", KEY.ESC],
  ["Enter", KEY.ENTER],
  ["Tab", KEY.TAB],
  ["Backspace", KEY.BACKSPACE],
  ["F1", KEY.F1],
  ["F2", KEY.F2],
  ["F3", KEY.F3],
  ["F4", KEY.F4],
  ["F5", KEY.F5],
  ["F6", KEY.F6],
  ["F7", KEY.F7],
  ["F8", KEY.F8],
  ["F9", KEY.F9],
  ["F10", KEY.F10],
  ["F11", KEY.F11],
]);

const canvas = document.getElementById("screen") as HTMLCanvasElement;
const statusEl = document.getElementById("status") as HTMLSpanElement;
const wadFileInput = document.getElementById("wad-file") as HTMLInputElement;
const wadPathInput = document.getElementById("wad-path") as HTMLInputElement;
const bootBtn = document.getElementById("boot-btn") as HTMLButtonElement;
const pointerBtn = document.getElementById("pointer-btn") as HTMLButtonElement;
const scaleSlider = document.getElementById("scale") as HTMLInputElement;
const sfxSlider = document.getElementById("sfx-volume") as HTMLInputElement;
const musicSlider = document.getElementById("music-volume") as HTMLInputElement;
const aspectCheckbox = document.getElementById("aspect-correct") as HTMLInputElement;

const canvasCtx = canvas.getContext("2d", { alpha: false });
if (!canvasCtx) {
  throw new Error("Canvas 2D not available");
}
const ctx: CanvasRenderingContext2D = canvasCtx;
ctx.imageSmoothingEnabled = false;

let exportsRef: EngineExports | null = null;
let memory: WebAssembly.Memory | null = null;
let preopenRoot: PreopenDirectory | null = null;
let imageData: ImageData | null = null;
let rafHandle = 0;
let accum = 0;
let lastTs = 0;
let audioCtx: AudioContext | null = null;
let scriptNode: ScriptProcessorNode | null = null;
let musicGain: Tone.Gain | null = null;
let songCache = new Map<number, Uint8Array>();
let activeSongTimeout: number | null = null;
const gamepadPressed = new Map<number, boolean>();

function setStatus(msg: string): void {
  statusEl.textContent = msg;
}

function b64Encode(bytes: Uint8Array): string {
  let s = "";
  for (let i = 0; i < bytes.length; i += 1) {
    s += String.fromCharCode(bytes[i]);
  }
  return btoa(s);
}

function b64Decode(text: string): Uint8Array {
  const raw = atob(text);
  const out = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i += 1) {
    out[i] = raw.charCodeAt(i);
  }
  return out;
}

function serializeDirectory(dir: Directory): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [name, inode] of dir.contents.entries()) {
    if (inode instanceof File) {
      out[name] = b64Encode(inode.data);
    }
  }
  return out;
}

function restoreDirectory(entries: Record<string, string>): Directory {
  const map = new Map<string, File>();
  for (const [name, value] of Object.entries(entries)) {
    map.set(name, new File(b64Decode(value)));
  }
  return new Directory(map);
}

function persistFs(): void {
  if (!preopenRoot) {
    return;
  }
  const payload = serializeDirectory(preopenRoot.dir);
  localStorage.setItem(STORAGE_KEY, JSON.stringify(payload));
}

function loadFs(): Directory {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (!raw) {
    return new Directory(new Map());
  }
  try {
    const parsed = JSON.parse(raw) as Record<string, string>;
    return restoreDirectory(parsed);
  } catch {
    return new Directory(new Map());
  }
}

function applyCanvasScale(): void {
  const scale = Number(scaleSlider.value);
  const width = 640 * scale;
  const height = (aspectCheckbox.checked ? 480 : 400) * scale;
  canvas.style.width = `${width}px`;
  canvas.style.height = `${height}px`;
}

function writeBytesToWasm(bytes: Uint8Array): { ptr: number; len: number } {
  if (!exportsRef || !memory) {
    throw new Error("WASM not initialized");
  }
  const ptr = exportsRef.alloc(bytes.length);
  const mem = new Uint8Array(memory.buffer, ptr, bytes.length);
  mem.set(bytes);
  return { ptr, len: bytes.length };
}

function maybeAsciiCode(ev: KeyboardEvent): number | null {
  if (ev.key.length === 1) {
    const code = ev.key.charCodeAt(0);
    if (code >= 32 && code <= 126) {
      return code | 0;
    }
  }
  if (ev.key === "-") return 0x2d;
  if (ev.key === "=") return 0x3d;
  return null;
}

function pushKeyEvent(ev: KeyboardEvent, pressed: boolean): void {
  if (!exportsRef) {
    return;
  }
  const mapped = keyMap.get(ev.code) ?? maybeAsciiCode(ev);
  if (mapped == null) {
    return;
  }
  exportsRef.queue_key_event(pressed ? 1 : 0, mapped);
  ev.preventDefault();
}

function pushVirtualKey(code: number, pressed: boolean): void {
  if (!exportsRef) {
    return;
  }
  exportsRef.queue_key_event(pressed ? 1 : 0, code);
}

function setGamepadKey(code: number, pressed: boolean): void {
  const prev = gamepadPressed.get(code) ?? false;
  if (prev === pressed) {
    return;
  }
  gamepadPressed.set(code, pressed);
  pushVirtualKey(code, pressed);
}

function pollGamepad(): void {
  const pads = navigator.getGamepads?.();
  if (!pads || !exportsRef) {
    return;
  }
  const gp = pads[0];
  if (!gp) {
    return;
  }

  const x = gp.axes[0] ?? 0;
  const y = gp.axes[1] ?? 0;
  const dead = 0.3;

  setGamepadKey(KEY.LEFT, x < -dead || !!gp.buttons[14]?.pressed);
  setGamepadKey(KEY.RIGHT, x > dead || !!gp.buttons[15]?.pressed);
  setGamepadKey(KEY.UP, y < -dead || !!gp.buttons[12]?.pressed);
  setGamepadKey(KEY.DOWN, y > dead || !!gp.buttons[13]?.pressed);
  setGamepadKey(KEY.FIRE, !!gp.buttons[0]?.pressed || !!gp.buttons[7]?.pressed);
  setGamepadKey(KEY.USE, !!gp.buttons[1]?.pressed);
  setGamepadKey(KEY.RSHIFT, !!gp.buttons[4]?.pressed);
  setGamepadKey(KEY.RALT, !!gp.buttons[5]?.pressed);
}

function buttonMask(e: MouseEvent): number {
  return e.buttons & 0x7;
}

function startAudio(): void {
  if (!exportsRef || !memory || audioCtx) {
    return;
  }

  audioCtx = new AudioContext({ sampleRate: 44100 });
  const node = audioCtx.createScriptProcessor(1024, 0, 2);
  scriptNode = node;
  node.onaudioprocess = (ev) => {
    if (!exportsRef || !memory) {
      return;
    }

    const outL = ev.outputBuffer.getChannelData(0);
    const outR = ev.outputBuffer.getChannelData(1);

    const ringPtr = exportsRef.audio_ringbuffer_ptr();
    const ringLen = exportsRef.audio_ringbuffer_len();
    const readPtr = exportsRef.audio_read_index_ptr();
    const writePtr = exportsRef.audio_write_index_ptr();

    if (!ringPtr || !readPtr || !writePtr || ringLen < 2) {
      outL.fill(0);
      outR.fill(0);
      return;
    }

    const ring = new Float32Array(memory.buffer, ringPtr, ringLen);
    const readIdxMem = new Uint32Array(memory.buffer, readPtr, 1);
    const writeIdxMem = new Uint32Array(memory.buffer, writePtr, 1);

    const capFrames = (ringLen / 2) >>> 0;
    let r = readIdxMem[0] >>> 0;
    const w = writeIdxMem[0] >>> 0;

    for (let i = 0; i < outL.length; i += 1) {
      if (r === w) {
        outL[i] = 0;
        outR[i] = 0;
      } else {
        const base = (r * 2) % ringLen;
        outL[i] = ring[base];
        outR[i] = ring[base + 1];
        r = (r + 1) % capFrames;
      }
    }

    readIdxMem[0] = r;
  };
  node.connect(audioCtx.destination);

  void Tone.start();
  musicGain = new Tone.Gain(Number(musicSlider.value) / 100).toDestination();
  setStatus("Audio ready");
}

function stopMusicPlayback(): void {
  if (activeSongTimeout != null) {
    window.clearTimeout(activeSongTimeout);
    activeSongTimeout = null;
  }
  Tone.Transport.stop();
  Tone.Transport.cancel();
}

async function playMidiBytes(bytes: Uint8Array, loop: boolean): Promise<void> {
  if (!musicGain) {
    return;
  }

  stopMusicPlayback();

  const midi = new Midi(bytes);
  const synth = new Tone.PolySynth(Tone.Synth).connect(musicGain);
  const now = Tone.now() + 0.05;

  for (const track of midi.tracks) {
    for (const note of track.notes) {
      synth.triggerAttackRelease(note.name, note.duration, now + note.time, note.velocity);
    }
  }

  if (loop && midi.duration > 0) {
    activeSongTimeout = window.setTimeout(() => {
      void playMidiBytes(bytes, true);
    }, (midi.duration + 0.1) * 1000);
  }
}

async function pollMusicEvents(): Promise<void> {
  if (!exportsRef || !memory) {
    return;
  }

  while (true) {
    const kind = exportsRef.music_pop_event();
    if (kind === 0) {
      break;
    }

    const songId = exportsRef.music_event_song_id();
    const value = exportsRef.music_event_value();

    if (kind === 1) {
      const ptr = exportsRef.music_event_data_ptr();
      const len = exportsRef.music_event_data_len();
      if (ptr && len > 0) {
        const bytes = new Uint8Array(memory.buffer, ptr, len);
        songCache.set(songId, new Uint8Array(bytes));
      }
    } else if (kind === 2) {
      songCache.delete(songId);
    } else if (kind === 3) {
      const bytes = songCache.get(songId);
      if (bytes) {
        await playMidiBytes(bytes, value !== 0);
      }
    } else if (kind === 4) {
      stopMusicPlayback();
    } else if (kind === 5) {
      if (value !== 0) {
        Tone.Transport.pause();
      } else {
        Tone.Transport.start();
      }
    } else if (kind === 6) {
      if (musicGain) {
        musicGain.gain.value = Math.max(0, Math.min(1, value / 127));
      }
    }
  }
}

function blitFrame(): void {
  if (!exportsRef || !memory) {
    return;
  }
  const ptr = exportsRef.framebuffer_ptr();
  const len = exportsRef.framebuffer_len();
  const width = exportsRef.framebuffer_width();
  const height = exportsRef.framebuffer_height();
  if (!ptr || len === 0 || width === 0 || height === 0) {
    return;
  }

  if (!imageData || imageData.width !== width || imageData.height !== height) {
    imageData = ctx.createImageData(width, height);
  }

  const src = new Uint32Array(memory.buffer, ptr, len);
  const dst = new Uint32Array(imageData.data.buffer);
  dst.set(src);
  ctx.putImageData(imageData, 0, 0);
}

async function frame(ts: number): Promise<void> {
  if (!exportsRef) {
    return;
  }
  pollGamepad();

  const dt = lastTs > 0 ? ts - lastTs : 0;
  lastTs = ts;
  accum += dt;

  let tics = 0;
  while (accum >= TIC_MS && tics < 8) {
    accum -= TIC_MS;
    tics += 1;
  }

  if (tics > 0) {
    exportsRef.run_tics(tics);
  }

  blitFrame();
  await pollMusicEvents();

  rafHandle = requestAnimationFrame((next) => {
    void frame(next);
  });
}

async function bootFromWadBytes(wadBytes: Uint8Array, virtualPath: string): Promise<void> {
  if (rafHandle) {
    cancelAnimationFrame(rafHandle);
    rafHandle = 0;
  }

  songCache.clear();
  stopMusicPlayback();

  const fsRoot = loadFs();
  fsRoot.contents.set(virtualPath, new File(wadBytes, { readonly: true }));
  preopenRoot = new PreopenDirectory(".", fsRoot.contents);

  const fds = [
    new OpenFile(new File([])),
    ConsoleStdout.lineBuffered((line) => console.log(`[doom] ${line}`)),
    ConsoleStdout.lineBuffered((line) => console.warn(`[doom-err] ${line}`)),
    preopenRoot,
  ];

  const wasiCtx = new WASI(["doom"], [], fds);
  const wasmResponse = await fetch("/engine_wasm.wasm");
  const wasmBytes = await wasmResponse.arrayBuffer();
  const module = await WebAssembly.compile(wasmBytes);
  const instance = (await WebAssembly.instantiate(module, {
    wasi_snapshot_preview1: wasiCtx.wasiImport,
  })) as unknown as { exports: EngineExports };

  exportsRef = instance.exports;
  memory = exportsRef.memory;

  startAudio();

  const configObj = {
    iwad_virtual_path: virtualPath,
    args: ["doom", "-iwad", virtualPath],
  };
  const configBytes = new TextEncoder().encode(JSON.stringify(configObj));

  const wadAlloc = writeBytesToWasm(wadBytes);
  const cfgAlloc = writeBytesToWasm(configBytes);

  const rc = exportsRef.init(cfgAlloc.ptr, cfgAlloc.len, wadAlloc.ptr, wadAlloc.len);
  exportsRef.dealloc(cfgAlloc.ptr, cfgAlloc.len);
  exportsRef.dealloc(wadAlloc.ptr, wadAlloc.len);

  if (rc !== 0) {
    setStatus(`Engine init failed (${rc})`);
    return;
  }

  exportsRef.set_sfx_master_volume(Number(sfxSlider.value) / 100);

  setStatus(`Running (${virtualPath})`);
  lastTs = 0;
  accum = 0;
  applyCanvasScale();
  rafHandle = requestAnimationFrame((next) => {
    void frame(next);
  });

  persistFs();
}

async function bootFromUI(): Promise<void> {
  const file = wadFileInput.files?.[0];
  const path = (wadPathInput.value || "DOOM.WAD").trim();

  if (file) {
    const bytes = new Uint8Array(await file.arrayBuffer());
    localStorage.setItem(WAD_STORAGE_KEY, b64Encode(bytes));
    await bootFromWadBytes(bytes, path);
    return;
  }

  const cached = localStorage.getItem(WAD_STORAGE_KEY);
  if (cached) {
    await bootFromWadBytes(b64Decode(cached), path);
    return;
  }

  setStatus("Choose a WAD file or boot cached WAD");
}

window.addEventListener("keydown", (ev) => pushKeyEvent(ev, true));
window.addEventListener("keyup", (ev) => pushKeyEvent(ev, false));

canvas.addEventListener("mousedown", (ev) => {
  if (exportsRef) {
    exportsRef.queue_mouse_event(buttonMask(ev), 0, 0);
  }
});
canvas.addEventListener("mouseup", (ev) => {
  if (exportsRef) {
    exportsRef.queue_mouse_event(buttonMask(ev), 0, 0);
  }
});
canvas.addEventListener("mousemove", (ev) => {
  if (exportsRef && document.pointerLockElement === canvas) {
    exportsRef.queue_mouse_event(buttonMask(ev), ev.movementX | 0, ev.movementY | 0);
  }
});

pointerBtn.addEventListener("click", () => {
  void canvas.requestPointerLock();
});

bootBtn.addEventListener("click", () => {
  void bootFromUI();
});

scaleSlider.addEventListener("input", applyCanvasScale);
aspectCheckbox.addEventListener("change", applyCanvasScale);
sfxSlider.addEventListener("input", () => {
  if (exportsRef) {
    exportsRef.set_sfx_master_volume(Number(sfxSlider.value) / 100);
  }
});
musicSlider.addEventListener("input", () => {
  if (musicGain) {
    musicGain.gain.value = Number(musicSlider.value) / 100;
  }
});

window.addEventListener("beforeunload", () => {
  persistFs();
});

window.setInterval(() => {
  persistFs();
}, 5000);

applyCanvasScale();
setStatus("Ready: pick WAD and boot");
