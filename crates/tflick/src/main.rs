//! Runs `$SHELL` inside a PTY, passing all I/O through untouched, and feeds a copy of it to
//! [`popup::Popup`] which tells `tflick-ui` where the cursor is.
//!
//! Modules: `main` wires the threads; `tty` is the real terminal (all `unsafe` lives there);
//! `popup` decides what to show; `screen` models the screen and reads the edit buffer, from
//! `osc` (OSC 697 splitting) and `shell` (its events); `cpr` parses cursor reports;
//! `ui_client` talks to `tflick-ui`.
//!
//! `tflick init zsh` prints the shell integration, for `eval "$(tflick init zsh)"` in .zshrc.

mod cpr;
mod osc;
mod popup;
mod screen;
mod shell;
mod tty;
mod ui_client;

use std::io::{self, Read, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, process, thread};

use anyhow::Result;
use portable_pty::{CommandBuilder, native_pty_system};
use signal_hook::consts::SIGWINCH;
use signal_hook::iterator::Signals;

fn main() -> Result<()> {
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let args: Vec<_> = env::args_os().skip(1).collect();

    if args.first().is_some_and(|a| a == "init") {
        return match args.get(1).and_then(|s| s.to_str()) {
            Some("zsh") => {
                print!("{}", include_str!("../../../shell/tflick.zsh"));
                Ok(())
            }
            other => anyhow::bail!("tflick init: unsupported shell {other:?} (only zsh for now)"),
        };
    }

    // Already inside tflick: become the shell instead of nesting a second PTY. Compare ttys, not
    // just an env var: TFLICK* vars leak into anything launched from a tflick shell (e.g. `code .`
    // gives every VSCode terminal TFLICK=1), and those other terminals must still get tflick.
    let outer_tty = env::var_os("TFLICK_TTY").map(PathBuf::from);
    if outer_tty.is_some() && outer_tty == tty::stdin_path() {
        // Say so: otherwise a freshly built tflick run inside an old one looks like it does
        // nothing new, since the outer (old) one keeps driving the popup.
        eprintln!(
            "tflick: already running in this terminal (session {}); starting a plain shell. \
             Type `exit` to leave it, or open a new terminal window to use this build.",
            env::var("TFLICK_SESSION_ID").unwrap_or_default()
        );
        return Err(process::Command::new(&shell).args(&args).exec().into());
    }

    let pair = native_pty_system().openpty(tty::size())?;

    let mut cmd = CommandBuilder::new(&shell);
    cmd.args(&args);
    // portable-pty defaults cwd to $HOME, keep the caller's directory instead.
    cmd.cwd(env::current_dir()?);
    cmd.env("TFLICK", "1");
    if let Some(tty) = pair.master.tty_name() {
        cmd.env("TFLICK_TTY", tty);
    }
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let session = format!("{}-{nanos}", process::id());
    cmd.env("TFLICK_SESSION_ID", &session);

    let raw = tty::RawMode::enable()?;
    // Ask the real terminal where its cursor is (DSR). Must happen before the stdin thread starts
    // reading (the answer arrives on stdin) and before the shell starts (a slow or missing answer
    // times out after 2s, and a shell that exits meanwhile would lose its output).
    let start_cursor = crossterm::cursor::position().ok();
    let popup = Arc::new(Mutex::new(popup::Popup::new(
        session,
        tty::size(),
        start_cursor,
    )));

    let mut child = pair.slave.spawn_command(cmd)?;
    // Drop our copy of the slave so the reader sees EOF once the shell exits.
    drop(pair.slave);

    let master = pair.master;
    let mut reader = master.try_clone_reader()?;
    let mut writer = master.take_writer()?;

    // stdin -> pty. Blocks on read forever; the process exits without joining it.
    let input_popup = popup.clone();
    thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut buf = [0u8; 4096];
        while let Ok(n) = stdin.read(&mut buf) {
            if n == 0 {
                break;
            }
            // Strips the terminal's cursor position answers before they reach the shell.
            let keys = input_popup.lock().unwrap().on_input(&buf[..n]);
            if writer.write_all(&keys).is_err() {
                break;
            }
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
            // Ask the terminal where its cursor is now; the answer arrives on stdin.
            if output_popup.lock().unwrap().on_output(&buf[..n])
                && stdout
                    .write_all(cpr::CURSOR_REQUEST)
                    .and_then(|_| stdout.flush())
                    .is_err()
            {
                break;
            }
        }
        let _ = done_tx.send(());
    });

    // Forward terminal resizes to the shell.
    let mut signals = Signals::new([SIGWINCH])?;
    thread::spawn(move || {
        for _ in signals.forever() {
            let size = tty::size();
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
