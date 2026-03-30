# ferrum-doom

Browser-playable Doom runtime using a Rust/WASM engine boundary and a minimal TypeScript host.

## Layout

- `engine/` Rust workspace with Doom core integration and WASM exports.
- `platform_web/` Vite + TypeScript browser host.
- `assets/` placeholder only (no bundled copyrighted WADs).
- `docs/` architecture and validation guides.

## Quick Start

```bash
npm install
npm run dev
```

Production build:

```bash
npm run build
```

Rust tests:

```bash
cargo test --manifest-path engine/Cargo.toml
```

## WADs

No WAD files are included. Use your own shareware/registered IWAD with the browser file picker.
