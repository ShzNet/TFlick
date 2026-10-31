//! Tracks the screen (cursor position) and tells `tflick-ui` where to show the popup.
//!
//! The cursor position comes from the real terminal itself: after shell output we ask it
//! (DSR `ESC[6n`) and it answers on stdin (CPR `ESC[row;colR`), which we strip before the shell
//! sees it. Our own screen model (alacritty_terminal) can drift from the real screen (content
//! from before we started, resize reflow, `clear`), so it is only the fallback for terminals that
//! never answer, plus the source of alternate-screen state.

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
    /// Cursor position requests sent to the terminal and not answered yet.
    cpr_outstanding: u32,
    cpr_answered: bool,
}

/// Unanswered requests after which we assume the terminal doesn't support them.
const CPR_GIVE_UP: u32 = 8;

/// Asks the terminal for its cursor position; write this to the real terminal (stdout).
pub const CURSOR_REQUEST: &[u8] = b"\x1b[6n";

impl Popup {
    /// `cursor` is where the real terminal's cursor is when we start, (column, row): the screen
    /// usually already has content above (login banner, earlier commands).
    pub fn new(session: String, size: PtySize, cursor: Option<(u16, u16)>) -> Self {
        let dims = TermSize::new(size.cols.into(), size.rows.into());
        let mut term = Term::new(Config::default(), &dims, VoidListener);
        let mut parser = Processor::new();
        if let Some((col, row)) = cursor {
            // CUP is 1-based.
            parser.advance(
                &mut term,
                format!("\x1b[{};{}H", row + 1, col + 1).as_bytes(),
            );
        }
        Self {
            session,
            term,
            parser,
            size,
            active: false,
            last_cursor: None,
            conn: None,
            cpr_outstanding: 0,
            cpr_answered: false,
        }
    }

    pub fn resize(&mut self, size: PtySize) {
        self.term
            .resize(TermSize::new(size.cols.into(), size.rows.into()));
        self.size = size;
        self.last_cursor = None;
    }

    /// Called with every chunk read from stdin. Strips the terminal's answers to our cursor
    /// requests and returns the bytes to forward to the shell.
    pub fn on_input(&mut self, bytes: &[u8]) -> Vec<u8> {
        // Only look for answers while one is due: Shift+F3 sends the same `ESC[1;2R` shape.
        if self.cpr_outstanding == 0 {
            self.on_keys(bytes);
            return bytes.to_vec();
        }
        let (keys, answers) = take_cursor_reports(bytes, self.cpr_outstanding);
        self.cpr_outstanding -= answers.len() as u32;
        if let Some(&cursor) = answers.last() {
            self.cpr_answered = true;
            if self.should_show() {
                self.show(cursor);
            }
        }
        if !keys.is_empty() {
            self.on_keys(&keys);
        }
        keys
    }

    /// ponytail: guesses "typing a command" from raw keys; replace with OSC 697 prompt state (P1).
    fn on_keys(&mut self, bytes: &[u8]) {
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
    /// Returns true when the caller should write [`CURSOR_REQUEST`] to the real terminal.
    pub fn on_output(&mut self, bytes: &[u8]) -> bool {
        self.parser.advance(&mut self.term, bytes);
        if !self.should_show() {
            return false;
        }
        if self.cpr_answered || self.cpr_outstanding < CPR_GIVE_UP {
            // One request per output chunk: answers come back in order, the last one wins.
            self.cpr_outstanding += 1;
            return true;
        }
        // The terminal never answers: fall back to our own screen model.
        let point = self.term.grid().cursor.point;
        self.show((point.column.0 as u16, point.line.0.max(0) as u16));
        false
    }

    /// Full-screen apps (vim, less, htop) use the alternate screen: never show there.
    fn should_show(&self) -> bool {
        self.active && !self.term.mode().contains(TermMode::ALT_SCREEN)
    }

    fn show(&mut self, cursor: (u16, u16)) {
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

/// Removes up to `max` cursor position reports (`ESC[row;colR`, 1-based) from `bytes`.
/// Returns the remaining bytes and the reported positions as 0-based (column, row).
/// ponytail: a report split across two reads is passed through to the shell; never seen in
/// practice since terminals write the few bytes at once.
fn take_cursor_reports(bytes: &[u8], max: u32) -> (Vec<u8>, Vec<(u16, u16)>) {
    let mut rest = Vec::with_capacity(bytes.len());
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if (found.len() as u32) < max
            && let Some((len, pos)) = parse_cursor_report(&bytes[i..])
        {
            found.push(pos);
            i += len;
        } else {
            rest.push(bytes[i]);
            i += 1;
        }
    }
    (rest, found)
}

/// `ESC [ row ; col R` at the start of `bytes` -> (length, (col - 1, row - 1)).
fn parse_cursor_report(bytes: &[u8]) -> Option<(usize, (u16, u16))> {
    let body = bytes.strip_prefix(b"\x1b[")?;
    let end = body.iter().position(|&b| b == b'R')?;
    let (row, col) = std::str::from_utf8(&body[..end]).ok()?.split_once(';')?;
    let (row, col): (u16, u16) = (row.parse().ok()?, col.parse().ok()?);
    Some((2 + end + 1, (col.saturating_sub(1), row.saturating_sub(1))))
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
        let mut p = Popup::new("s".into(), size(), None);
        p.parser.advance(&mut p.term, b"line1\r\n$ git co");
        let point = p.term.grid().cursor.point;
        assert_eq!((point.column.0, point.line.0), (8, 1));
    }

    #[test]
    fn starts_at_the_real_cursor_position() {
        let mut p = Popup::new("s".into(), size(), Some((0, 8)));
        p.parser.advance(&mut p.term, b"$ hi");
        let point = p.term.grid().cursor.point;
        assert_eq!((point.column.0, point.line.0), (4, 8));
    }

    #[test]
    fn enter_deactivates_and_typing_activates() {
        let mut p = Popup::new("s".into(), size(), None);
        p.on_input(b"g");
        assert!(p.active);
        p.on_input(b"\r");
        assert!(!p.active);
        p.on_input(b"\x1b[A"); // arrow up browses history, not typing
        assert!(!p.active);
    }

    #[test]
    fn alt_screen_never_shows() {
        let mut p = Popup::new("s".into(), size(), None);
        p.on_input(b"x");
        let asks = p.on_output(b"\x1b[?1049h"); // enter alternate screen, like vim
        assert!(!asks);
        assert_eq!(p.last_cursor, None);
    }

    #[test]
    fn cursor_comes_from_the_terminal_answer_which_the_shell_never_sees() {
        let mut p = Popup::new("s".into(), size(), None);
        p.on_input(b"h");
        assert!(p.on_output(b"h"));
        // The answer arrives together with the next keystroke.
        let fwd = p.on_input(b"\x1b[11;23Ri");
        assert_eq!(fwd, b"i");
        assert_eq!(p.last_cursor, Some((22, 10)));
    }

    #[test]
    fn shift_f3_passes_through_when_no_answer_is_due() {
        let mut p = Popup::new("s".into(), size(), None);
        assert_eq!(p.on_input(b"\x1b[1;2R"), b"\x1b[1;2R");
    }

    #[test]
    fn falls_back_to_the_model_when_the_terminal_never_answers() {
        let mut p = Popup::new("s".into(), size(), Some((0, 5)));
        p.on_input(b"x");
        for _ in 0..CPR_GIVE_UP {
            assert!(p.on_output(b"x"));
        }
        assert!(!p.on_output(b"y"));
        assert_eq!(p.last_cursor, Some((CPR_GIVE_UP as u16 + 1, 5))); // 8 x + y
    }

    #[test]
    fn parses_cursor_reports() {
        let (rest, found) = take_cursor_reports(b"a\x1b[3;4Rb\x1b[10;1R\x1b[A", 5);
        assert_eq!(rest, b"ab\x1b[A");
        assert_eq!(found, vec![(3, 2), (0, 9)]);
        let (rest, found) = take_cursor_reports(b"\x1b[3;4R", 0);
        assert_eq!((rest.as_slice(), found.len()), (&b"\x1b[3;4R"[..], 0));
    }
}
