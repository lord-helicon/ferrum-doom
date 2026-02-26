#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

echo "[env] installing Rust stable toolchain"
rustup toolchain install stable --profile minimal --component rustfmt --component clippy
rustup default stable

STABLE_RUSTC_VERSION="$(rustc +stable --version | awk '{print $2}')"
STABLE_MAJOR="$(echo "$STABLE_RUSTC_VERSION" | cut -d. -f1)"
STABLE_MINOR="$(echo "$STABLE_RUSTC_VERSION" | cut -d. -f2)"
if [[ "$STABLE_MAJOR" -lt 1 ]] || [[ "$STABLE_MAJOR" -eq 1 && "$STABLE_MINOR" -lt 93 ]]; then
  echo "[env] expected Rust stable >= 1.93.x, got $STABLE_RUSTC_VERSION" >&2
  exit 1
fi

echo "[env] adding wasm32-wasip1 Rust target"
rustup target add wasm32-wasip1

echo "[env] prewarming Cargo dependency graph (host + wasm)"
cargo fetch --manifest-path "$ROOT_DIR/engine/Cargo.toml"
cargo fetch --manifest-path "$ROOT_DIR/engine/Cargo.toml" --target wasm32-wasip1

echo "[env] prewarming engine workspace build graph"
cargo check --manifest-path "$ROOT_DIR/engine/Cargo.toml"

echo "[env] installing npm workspace dependencies"
if [[ -f "$ROOT_DIR/package-lock.json" ]]; then
  npm ci --prefix "$ROOT_DIR" --no-audit --no-fund
else
  npm install --prefix "$ROOT_DIR" --no-audit --no-fund
fi

echo "[env] prewarming wasm build pipeline"
bash "$ROOT_DIR/scripts/build_wasm.sh"

echo "[env] prewarming web build pipeline"
npm run build --prefix "$ROOT_DIR"

echo "[env] cloud setup completed"
