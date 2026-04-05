# Agent Guidelines for ferrum-doom

A browser-playable Doom runtime using a Rust/WASM engine with a TypeScript host.

## Build Commands

### Rust (Engine)
```bash
# Build entire engine workspace
cargo build --manifest-path engine/Cargo.toml

# Build WASM module for browser
npm run build:wasm

# Build release
cargo build --manifest-path engine/Cargo.toml --release

# Build specific crate
cargo build -p engine_core
```

### TypeScript/Web
```bash
# Install dependencies
npm install

# Development (builds WASM first, then Vite dev server)
npm run dev

# Production build
npm run build

# TypeScript check
tsc -b --project platform_web/tsconfig.json
```

### Testing
```bash
# Run all Rust tests
cargo test --manifest-path engine/Cargo.toml

# Run tests for specific crate
cargo test -p gameplay

# Run single test file
cargo test --manifest-path engine/Cargo.toml --lib r4d.gameplay.src.level.tests.bsp3d_e1m1_tests

# Run single test function (partial match)
cargo test --manifest-path engine/Cargo.toml -- bsp3d_e1m1
```

## Code Style

### Rust

- **Edition**: 2021 (see `engine/Cargo.toml`)
- **Imports**: Group by std → external → internal. Use `use` statements freely to bring items to scope. Prefer full paths in `use` (e.g., `use gameplay::tic_cmd::TicCmd`)
- **Error Handling**: Use `anyhow::Result<T>` for fallible operations. Use `thiserror` for specific error types in libraries. Propagate errors with `?` operator
- **Naming**: `CamelCase` for types/traits, `snake_case` for variables/functions, `SCREAMING_SNAKE_CASE` for constants
- **Unsafe**: `#![allow(unsafe_code)]` is used in engine_core. Mark unsafe blocks explicitly
- **Documentation**: Use doc comments (`///`) for public API. Markdown code blocks in doc comments with `ignore` annotation for diagrams
- **Formatting**: Run `cargo fmt` before committing. No rustfmt.toml configured (uses defaults)
- **Lints**: Run `cargo clippy` before committing. No custom clippy.toml

### TypeScript

- **Strict mode**: Enabled in tsconfig.json
- **Naming**: `camelCase` for variables/functions, `PascalCase` for types/classes/interfaces
- **Imports**: Use absolute imports via path aliases if configured, otherwise relative
- **Error Handling**: Use typed errors. Avoid `any`. Prefer `unknown` for catch clause parameters
- **Formatting**: No prettier/eslint configured, but follow project conventions

## Architecture

```
engine/                    # Rust workspace
  engine_core/            # Core Doom logic, framebuffer
  engine_wasm/            # WASM bindings/exports
  engine_platform/        # Platform abstractions
  engine_render/          # Rendering trait
  engine_sound/           # Sound server trait
  engine_music/           # Music server trait
  engine_wad/             # WAD loading
  r4d/                    # "Room4Doom" derived code
    math/                 # Fixed point, trig, angles
    wad/                  # WAD parsing
    gameplay/             # Game logic, levels, entities
    render/software25d/   # Software renderer

platform_web/             # TypeScript/Vite browser host
```

## Important Patterns

- **Module Exports**: In `lib.rs`, re-export types at root level with `pub use crate::module::*;`
- **Traits**: Use traits for abstraction (e.g., `SoundServer`, `DrawBuffer`). Implement traits for different backends
- **WASM Interop**: `engine_wasm` crate exposes functions to JS. Keep FFI boundaries clean
- **Level Loading**: `Level::new_empty()` followed by `level.load()`. Things spawned separately via `MapObject::p_spawn_map_thing()`
- **Frame Buffer**: `SoftFrameBuffer` implements `DrawBuffer` trait. Uses RGBA u8 arrays
- **Glam**: The project uses `glam` crate for math (Vec2, Vec3, etc.)

## File Organization

- `src/lib.rs` - Crate root, exports public API, documentation
- `src/mod.rs` - Module declarations
- `src/` - Implementation files
- `tests/` - Integration tests in subdirectories
- `tests/*.rs` - Unit tests inline or in separate test modules

## Common Tasks

### Adding a new WASM export
1. Add function to `engine/engine_wasm/src/lib.rs`
2. Ensure function is `#[no_mangle]` and uses compatible types
3. Add to WASM build and copy step in `scripts/build_wasm.sh`

### Adding a new game environment feature (doors, platforms, etc.)
1. Add state to `Level` struct in `r4d/gameplay/src/level/mod.rs`
2. Implement thinking/special logic in `r4d/gameplay/src/env/`
3. Wire up in `update_specials()` function

### Running specific game logic
1. Create `TicCmd` with appropriate forwardmove/sidemove/angleturn
2. Pass to `Player::think()` which processes the command
3. Thinkers run via `level.thinkers.run_thinkers()`