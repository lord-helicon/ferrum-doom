use std::env;
use std::path::PathBuf;

fn should_skip_file(name: &str) -> bool {
    if name == "i_main.c" {
        return true;
    }

    if name.starts_with("doomgeneric_") {
        return true;
    }

    name.contains("_sdl")
        || name.contains("_allegro")
        || name.contains("_xlib")
        || name.contains("_win")
        || name.contains("_emscripten")
        || name.contains("_soso")
        || name.contains("_sosox")
}

fn maybe_download_wasi_sysroot() -> Option<PathBuf> {
    let target = env::var("TARGET").ok()?;
    if target != "wasm32-wasip1" {
        return None;
    }

    if let Ok(path) = env::var("WASI_SYSROOT") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    let candidate = PathBuf::from("../../_toolchains/wasi-sysroot/wasi-sysroot-25.0");
    if candidate.exists() {
        return Some(candidate);
    }

    eprintln!("cargo:warning=WASI sysroot not found in WASI_SYSROOT or ../../_toolchains/wasi-sysroot/wasi-sysroot-25.0");
    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let src_dir = PathBuf::from("c_src/doomgeneric");
    let mut c_files = Vec::new();

    for entry in std::fs::read_dir(&src_dir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };

        if !name.ends_with(".c") {
            continue;
        }

        if should_skip_file(name) {
            continue;
        }

        c_files.push(path);
    }

    c_files.sort();

    for path in &c_files {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    println!("cargo:rerun-if-changed=build.rs");

    let mut build = cc::Build::new();
    build.files(&c_files);
    build.include(&src_dir);
    build.flag("-w");
    build.define("DOOMGENERIC_RESX", "640");
    build.define("DOOMGENERIC_RESY", "400");

    if let Some(sysroot) = maybe_download_wasi_sysroot() {
        build.flag("--target=wasm32-wasi");
        build.flag(&format!("--sysroot={}", sysroot.display()));
    }

    if env::var("TARGET").ok().as_deref() == Some("wasm32-wasip1") {
        build.define("HAVE_UNISTD_H", None);
    }

    build.compile("doomcore");

    Ok(())
}
