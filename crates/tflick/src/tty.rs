//! The real terminal tflick runs in: raw mode, size, device path.

use std::ffi::CStr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use crossterm::terminal;
use portable_pty::PtySize;

/// Restores the outer terminal on drop, including on panic.
pub struct RawMode;

impl RawMode {
    pub fn enable() -> Result<Self> {
        terminal::enable_raw_mode().context("enable raw mode")?;
        Ok(Self)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

/// Size of the real terminal, for the PTY.
pub fn size() -> PtySize {
    // window_size also reports pixels, which the popup needs (cell size).
    match terminal::window_size() {
        // Some pseudo-terminals (e.g. `script`) report 0x0; an empty grid would panic later.
        Ok(s) if s.rows > 0 && s.columns > 0 => PtySize {
            rows: s.rows,
            cols: s.columns,
            pixel_width: s.width,
            pixel_height: s.height,
        },
        _ => PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
    }
}

/// Path of the terminal device on stdin, e.g. /dev/ttys003.
pub fn stdin_path() -> Option<PathBuf> {
    // SAFETY: ttyname returns null or a pointer to a static NUL-terminated buffer, copied out
    // immediately (we never call it from two threads at once: only at startup).
    unsafe {
        let p = libc::ttyname(0);
        (!p.is_null()).then(|| PathBuf::from(CStr::from_ptr(p).to_string_lossy().into_owned()))
    }
}
