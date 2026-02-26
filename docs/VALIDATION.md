# Validation

## Build and Run

From repository root:

```bash
npm install
npm run dev
```

Production build:

```bash
npm run build
```

Engine-only checks:

```bash
cargo check --manifest-path engine/Cargo.toml
cargo test --manifest-path engine/Cargo.toml
```

## Test Coverage

### 1. WAD parsing tests (`engine_wad`)

- Header/directory parsing
- Lump lookup
- Bounds validation and malformed input rejection

Run:

```bash
cargo test -p engine_wad --manifest-path engine/Cargo.toml
```

### 2. Determinism CRC hooks (`engine_wasm`)

Runtime exposes `current_state_crc()` for deterministic validation with fixed input streams.

Suggested harness procedure:

1. Load known IWAD.
2. Inject fixed key/mouse stream for `N` tics.
3. Record CRC at deterministic checkpoints.
4. Repeat and assert exact CRC match.

### 3. Render checksum points

Use `framebuffer_ptr/len` and compute CRC32 at checkpoints:

- Title screen after warmup tics.
- E1M1 first room after deterministic movement script.
- Sprite-dense combat scene checkpoint.

### 4. Audio checks

- SFX channel start/stop/pan validation via ringbuffer output.
- MUS->MIDI registration event emitted and music playback verified in host.

## Manual Play Validation Checklist

- [ ] IWAD loaded from file picker.
- [ ] Main menu interaction works.
- [ ] New game starts and map finishes are possible.
- [ ] Keyboard controls responsive.
- [ ] Pointer lock mouse look/turn works.
- [ ] SFX audible with volume control.
- [ ] Music plays, pauses, resumes.
- [ ] Save/load files persist across reload.
- [ ] Integer scaling and aspect-correct toggle render correctly.
