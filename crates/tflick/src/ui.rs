//! Tracks the screen (cursor position) and tells `tflick-ui` where to show the popup.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use alacritty_terminal::Term;
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use portable_pty::PtySize;
use tflick_ipc::{Item, Kind, ToUi};

pub struct Popup {
    session: String,
    term: Term<VoidListener>,
    parser: Processor,
    size: PtySize,
    /// The user is typing a command, so the popup should follow the cursor.
    active: bool,
    last_cursor: Option<(u16, u16)>,
    conn: Option<UnixStream>,
}

impl Popup {
    pub fn new(session: String, size: PtySize) -> Self {
        let dims = TermSize::new(size.cols.into(), size.rows.into());
        Self {
            session,
            term: Term::new(Config::default(), &dims, VoidListener),
            parser: Processor::new(),
            size,
            active: false,
            last_cursor: None,
            conn: None,
        }
    }

    pub fn resize(&mut self, size: PtySize) {
        self.term
            .resize(TermSize::new(size.cols.into(), size.rows.into()));
        self.size = size;
        self.last_cursor = None;
    }

    /// Called with every keystroke chunk the user sends to the shell.
    /// ponytail: guesses "typing a command" from raw keys; replace with OSC 697 prompt state (P1).
    pub fn on_input(&mut self, bytes: &[u8]) {
        let ends_command = bytes
            .iter()
            .any(|&b| matches!(b, b'\r' | b'\n' | 0x03 | 0x04))
            || bytes == b"\x1b";
        if ends_command {
            self.active = false;
            self.last_cursor = None;
            let msg = ToUi::Hide {
                session: self.session.clone(),
            };
            self.send(&msg);
        } else if bytes.first() != Some(&0x1b) && bytes.iter().any(|&b| b >= 0x20) {
            // Escape sequences (arrows, function keys) are navigation, not typing.
            self.active = true;
        }
    }

    /// Called with every chunk of shell output, after it was written to the real terminal.
    pub fn on_output(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
        // Full-screen apps (vim, less, htop) use the alternate screen: never show there.
        if !self.active || self.term.mode().contains(TermMode::ALT_SCREEN) {
            return;
        }
        let point = self.term.grid().cursor.point;
        let cursor = (point.column.0 as u16, point.line.0.max(0) as u16);
        if self.last_cursor == Some(cursor) {
            return;
        }
        self.last_cursor = Some(cursor);
        let s = self.size;
        let cell_px = (s.pixel_width > 0 && s.pixel_height > 0).then(|| {
            (
                s.pixel_width as f32 / s.cols as f32,
                s.pixel_height as f32 / s.rows as f32,
            )
        });
        let msg = ToUi::Show {
            session: self.session.clone(),
            items: fake_items(),
            selected: 0,
            cursor,
            grid: (s.cols, s.rows),
            cell_px,
        };
        self.send(&msg);
    }

    /// Never blocks the PTY for long: short write timeout, and a missing or dead UI is ignored
    /// (reconnected on the next message).
    fn send(&mut self, msg: &ToUi) {
        if self.conn.is_none() {
            self.conn = UnixStream::connect(tflick_ipc::socket_path()).ok();
            if let Some(c) = &self.conn {
                let _ = c.set_write_timeout(Some(Duration::from_millis(20)));
            }
        }
        let Some(conn) = &mut self.conn else { return };
        let mut line = serde_json::to_vec(msg).expect("ToUi serializes");
        line.push(b'\n');
        if conn.write_all(&line).is_err() {
            self.conn = None;
        }
    }
}

/// ponytail: fixed list until the parser and specs exist (P3/P4).
fn fake_items() -> Vec<Item> {
    let item = |kind, name: &str, desc: &str| Item {
        name: name.into(),
        description: Some(desc.into()),
        kind,
    };
    vec![
        item(
            Kind::Subcommand,
            "checkout",
            "Switch branches or restore working tree files",
        ),
        item(
            Kind::Subcommand,
            "commit",
            "Record changes to the repository",
        ),
        item(
            Kind::Option,
            "--amend",
            "Replace the tip of the current branch",
        ),
        item(
            Kind::Arg,
            "<pathspec>",
            "Limit the paths affected by the operation",
        ),
        item(Kind::Folder, "src/", "Folder"),
        item(Kind::File, "Cargo.toml", "File"),
        item(Kind::Branch, "main", "Branch"),
        item(Kind::History, "git commit -m \"wip\"", "From history"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size() -> PtySize {
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }
    }

    #[test]
    fn tracks_cursor_after_output() {
        let mut p = Popup::new("s".into(), size());
        p.parser.advance(&mut p.term, b"line1\r\n$ git co");
        let point = p.term.grid().cursor.point;
        assert_eq!((point.column.0, point.line.0), (8, 1));
    }

    #[test]
    fn enter_deactivates_and_typing_activates() {
        let mut p = Popup::new("s".into(), size());
        p.on_input(b"g");
        assert!(p.active);
        p.on_input(b"\r");
        assert!(!p.active);
        p.on_input(b"\x1b[A"); // arrow up browses history, not typing
        assert!(!p.active);
    }

    #[test]
    fn alt_screen_never_shows() {
        let mut p = Popup::new("s".into(), size());
        p.on_input(b"x");
        p.on_output(b"\x1b[?1049h"); // enter alternate screen, like vim
        assert_eq!(p.last_cursor, None);
    }
}
