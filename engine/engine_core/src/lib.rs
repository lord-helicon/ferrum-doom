#![allow(unsafe_code)]

use anyhow::{Context, Result};
use std::ffi::{c_char, c_int, CString};

unsafe extern "C" {
    fn doomgeneric_Create(argc: c_int, argv: *mut *mut c_char);
    fn doomgeneric_Tick();
    static mut DG_ScreenBuffer: *mut u32;
}

pub struct DoomCore {
    _argv_storage: Vec<CString>,
}

impl DoomCore {
    pub fn create(args: &[String]) -> Result<Self> {
        let mut argv_storage = Vec::with_capacity(args.len());
        for arg in args {
            argv_storage.push(
                CString::new(arg.as_str())
                    .with_context(|| format!("argument contains embedded NUL: {arg:?}"))?,
            );
        }

        let argv_ptrs: Vec<*mut c_char> = argv_storage
            .iter_mut()
            .map(|s| s.as_ptr() as *mut c_char)
            .collect();

        let argv_box = argv_ptrs.into_boxed_slice();
        let argv_ptr = argv_box.as_ptr() as *mut *mut c_char;
        // Intentional leak: Doom keeps `myargv` as a global pointer for process lifetime.
        Box::leak(argv_box);

        // SAFETY: pointers come from live CString storage in argv_storage.
        unsafe {
            doomgeneric_Create(argv_storage.len() as c_int, argv_ptr);
        }

        Ok(Self {
            _argv_storage: argv_storage,
        })
    }

    pub fn tick(&mut self) {
        // SAFETY: doomgeneric runtime initialized by create().
        unsafe {
            doomgeneric_Tick();
        }
    }

    pub fn framebuffer_ptr(&self) -> *const u32 {
        // SAFETY: screen buffer is allocated by doomgeneric_Create.
        unsafe { DG_ScreenBuffer as *const u32 }
    }
}
