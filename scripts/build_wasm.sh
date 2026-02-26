#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
ENGINE_DIR="$ROOT_DIR/engine"
OUT_DIR="$ROOT_DIR/platform_web/public"
WASM_OUT="$ENGINE_DIR/target/wasm32-wasip1/release/engine_wasm.wasm"
SYSROOT_DIR="$ROOT_DIR/_toolchains/wasi-sysroot/wasi-sysroot-25.0"

mkdir -p "$ROOT_DIR/_toolchains"
mkdir -p "$OUT_DIR"

if ! rustup target list --installed | rg -q "wasm32-wasip1"; then
  rustup target add wasm32-wasip1
fi

if [[ ! -d "$SYSROOT_DIR" ]]; then
  ARCHIVE="$ROOT_DIR/_toolchains/wasi-sysroot.tar.gz"
  mkdir -p "$ROOT_DIR/_toolchains/wasi-sysroot"
  curl -L --fail -o "$ARCHIVE" "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-25/wasi-sysroot-25.0.tar.gz"
  tar -xzf "$ARCHIVE" -C "$ROOT_DIR/_toolchains/wasi-sysroot"
fi

cd "$ENGINE_DIR"
CC_wasm32_wasip1=clang CFLAGS_wasm32_wasip1="--target=wasm32-wasi --sysroot=$SYSROOT_DIR" cargo build -p engine_wasm --target wasm32-wasip1 --release

cp "$WASM_OUT" "$OUT_DIR/engine_wasm.wasm"
echo "WASM copied to $OUT_DIR/engine_wasm.wasm"
