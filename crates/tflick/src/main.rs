//! Runs `$SHELL` inside a PTY, passing all I/O through untouched, and feeds a copy of it to
//! [`ui::Popup`] which tells `tflick-ui` where the cursor is.

mod ui;

use std::io::{self, Read, Write};
use std::os::unix::process::CommandExt;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, process, thread};

use anyhow::{Context, Result};
use crossterm::terminal;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use signal_hook::consts::SIGWINCH;
use signal_hook::iterator::Signals;

/// Restores the outer terminal on drop, including on panic.
struct RawMode;

impl RawMode {
    fn enable() -> Result<Self> {
        terminal::enable_raw_mode().context("enable raw mode")?;
        Ok(Self)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

fn pty_size() -> PtySize {
    // window_size also reports pixels, which the popup needs later (cell size).
    match terminal::window_size() {
        Ok(s) => PtySize {
            rows: s.rows,
            cols: s.columns,
            pixel_width: s.width,
            pixel_height: s.height,
        },
        Err(_) => PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
    }
}

fn main() -> Result<()> {
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let args: Vec<_> = env::args_os().skip(1).collect();

    // Already inside tflick: become the shell instead of nesting a second PTY.
    if env::var_os("TFLICK").is_some() {
        return Err(process::Command::new(&shell).args(&args).exec().into());
    }

    let pair = native_pty_system().openpty(pty_size())?;

    let mut cmd = CommandBuilder::new(&shell);
    cmd.args(&args);
    // portable-pty defaults cwd to $HOME, keep the caller's directory instead.
    cmd.cwd(env::current_dir()?);
    cmd.env("TFLICK", "1");
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let session = format!("{}-{nanos}", process::id());
    cmd.env("TFLICK_SESSION_ID", &session);

    let mut child = pair.slave.spawn_command(cmd)?;
    // Drop our copy of the slave so the reader sees EOF once the shell exits.
    drop(pair.slave);

    let master = pair.master;
    let mut reader = master.try_clone_reader()?;
    let mut writer = master.take_writer()?;

    let raw = RawMode::enable()?;
    // Ask the real terminal where its cursor is (DSR). Must happen before the stdin thread starts
    // reading, since the answer arrives on stdin. Times out (None) on terminals that don't reply.
    let start_cursor = crossterm::cursor::position().ok();
    let popup = Arc::new(Mutex::new(ui::Popup::new(
        session,
        pty_size(),
        start_cursor,
    )));

    // stdin -> pty. Blocks on read forever; the process exits without joining it.
    let input_popup = popup.clone();
    thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut buf = [0u8; 4096];
        while let Ok(n) = stdin.read(&mut buf) {
            if n == 0 || writer.write_all(&buf[..n]).is_err() {
                break;
            }
            input_popup.lock().unwrap().on_input(&buf[..n]);
        }
    });

    // pty -> stdout, then a copy to the popup tracker (the user sees output first).
    let (done_tx, done_rx) = mpsc::channel();
    let output_popup = popup.clone();
    thread::spawn(move || {
        let mut stdout = io::stdout().lock();
        let mut buf = [0u8; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0
                || stdout
                    .write_all(&buf[..n])
                    .and_then(|_| stdout.flush())
                    .is_err()
            {
                break;
            }
            output_popup.lock().unwrap().on_output(&buf[..n]);
        }
        let _ = done_tx.send(());
    });

    // Forward terminal resizes to the shell.
    let mut signals = Signals::new([SIGWINCH])?;
    thread::spawn(move || {
        for _ in signals.forever() {
            let size = pty_size();
            let _ = master.resize(size);
            popup.lock().unwrap().resize(size);
        }
    });

    let status = child.wait()?;
    // Flush the shell's last output. A background job holding the pty open must not hang us.
    let _ = done_rx.recv_timeout(Duration::from_millis(200));

    drop(raw);
    process::exit(status.exit_code() as i32);
}
