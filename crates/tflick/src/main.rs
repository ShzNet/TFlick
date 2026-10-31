//! P0: run `$SHELL` inside a PTY and pass all I/O through untouched.

use std::io::{self, Read, Write};
use std::os::unix::process::CommandExt;
use std::sync::mpsc;
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
        Ok(s) => PtySize { rows: s.rows, cols: s.columns, pixel_width: s.width, pixel_height: s.height },
        Err(_) => PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 },
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
    cmd.env("TFLICK_SESSION_ID", format!("{}-{nanos}", process::id()));

    let mut child = pair.slave.spawn_command(cmd)?;
    // Drop our copy of the slave so the reader sees EOF once the shell exits.
    drop(pair.slave);

    let master = pair.master;
    let mut reader = master.try_clone_reader()?;
    let mut writer = master.take_writer()?;

    let raw = RawMode::enable()?;

    // stdin -> pty. Blocks on read forever; the process exits without joining it.
    thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut buf = [0u8; 4096];
        while let Ok(n) = stdin.read(&mut buf) {
            if n == 0 || writer.write_all(&buf[..n]).is_err() {
                break;
            }
        }
    });

    // pty -> stdout.
    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut stdout = io::stdout().lock();
        let mut buf = [0u8; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 || stdout.write_all(&buf[..n]).and_then(|_| stdout.flush()).is_err() {
                break;
            }
        }
        let _ = done_tx.send(());
    });

    // Forward terminal resizes to the shell.
    let mut signals = Signals::new([SIGWINCH])?;
    thread::spawn(move || {
        for _ in signals.forever() {
            let _ = master.resize(pty_size());
        }
    });

    let status = child.wait()?;
    // Flush the shell's last output. A background job holding the pty open must not hang us.
    let _ = done_rx.recv_timeout(Duration::from_millis(200));

    drop(raw);
    process::exit(status.exit_code() as i32);
}
