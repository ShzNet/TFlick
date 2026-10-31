//! Decides when the popup shows and where, and tells `tflick-ui`.
//!
//! With the shell integration loaded, the popup shows while the edit buffer (see
//! [`crate::screen`]) has text. Without it, typing is guessed from raw keys.
//! The cursor position comes from the real terminal itself (see [`crate::cpr`]): our screen model
//! can drift from the real screen (content from before we started, resize reflow, `clear`), so
//! its cursor is only the fallback for terminals that never answer.

use std::fs::File;
use std::io::Write;

use portable_pty::PtySize;
use tflick_ipc::{Item, Kind, ToUi};

use crate::cpr::take_cursor_reports;
use crate::screen::{EditBuffer, Screen};
use crate::ui_client::UiClient;

pub struct Popup {
    session: String,
    screen: Screen,
    size: PtySize,
    /// Without the shell integration: the user seems to be typing a command.
    typing: bool,
    /// Where the popup was last shown; None while hidden.
    last_cursor: Option<(u16, u16)>,
    last_buffer: Option<EditBuffer>,
    ui: UiClient,
    /// Cursor position requests sent to the terminal and not answered yet.
    cpr_outstanding: u32,
    cpr_answered: bool,
    /// `TFLICK_LOG=/tmp/tflick.log`: edit buffer changes, as `text|cursor`.
    log: Option<File>,
}

/// Unanswered requests after which we assume the terminal doesn't support them.
const CPR_GIVE_UP: u32 = 8;

impl Popup {
    /// `cursor` is where the real terminal's cursor is when we start, (column, row).
    pub fn new(session: String, size: PtySize, cursor: Option<(u16, u16)>) -> Self {
        let log = std::env::var_os("TFLICK_LOG")
            .and_then(|path| File::options().create(true).append(true).open(path).ok());
        Self {
            screen: Screen::new(session.clone(), size.cols, size.rows, cursor),
            session,
            size,
            typing: false,
            last_cursor: None,
            last_buffer: None,
            ui: UiClient::default(),
            cpr_outstanding: 0,
            cpr_answered: false,
            log,
        }
    }

    pub fn resize(&mut self, size: PtySize) {
        self.screen.resize(size.cols, size.rows);
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

    /// Enter, Ctrl-C, Ctrl-D and a lone Esc end the command: hide right away rather than wait
    /// for the shell's output.
    /// ponytail: without the shell integration, "typing" is guessed from raw keys.
    fn on_keys(&mut self, bytes: &[u8]) {
        let ends_command = bytes
            .iter()
            .any(|&b| matches!(b, b'\r' | b'\n' | 0x03 | 0x04))
            || bytes == b"\x1b";
        if ends_command {
            self.typing = false;
            self.hide();
        } else if bytes.first() != Some(&0x1b) && bytes.iter().any(|&b| b >= 0x20) {
            // Escape sequences (arrows, function keys) are navigation, not typing.
            self.typing = true;
        }
    }

    /// Called with every chunk of shell output, after it was written to the real terminal.
    /// Returns true when the caller should write [`crate::cpr::CURSOR_REQUEST`] to the real
    /// terminal.
    pub fn on_output(&mut self, bytes: &[u8]) -> bool {
        self.screen.feed(bytes);
        self.log_buffer();
        if !self.should_show() {
            self.hide();
            return false;
        }
        if self.cpr_answered || self.cpr_outstanding < CPR_GIVE_UP {
            // One request per output chunk: answers come back in order, the last one wins.
            self.cpr_outstanding += 1;
            return true;
        }
        // The terminal never answers: fall back to our own screen model.
        self.show(self.screen.cursor());
        false
    }

    fn should_show(&self) -> bool {
        if self.screen.alt_screen() {
            return false;
        }
        if self.screen.integrated() {
            self.screen
                .edit_buffer()
                .is_some_and(|b| !b.text.trim().is_empty())
        } else {
            self.typing
        }
    }

    fn log_buffer(&mut self) {
        let Some(log) = &mut self.log else { return };
        let buffer = self.screen.edit_buffer();
        if buffer != self.last_buffer {
            let _ = match &buffer {
                Some(b) => writeln!(log, "{}|{}", b.text, b.cursor),
                None => writeln!(log, "(none)"),
            };
            self.last_buffer = buffer;
        }
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
        self.ui.send(&msg);
    }

    fn hide(&mut self) {
        if self.last_cursor.take().is_some() {
            let msg = ToUi::Hide {
                session: self.session.clone(),
            };
            self.ui.send(&msg);
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
    fn starts_at_the_real_cursor_position() {
        let mut p = Popup::new("s".into(), size(), Some((0, 8)));
        p.on_output(b"$ hi");
        assert_eq!(p.screen.cursor(), (4, 8));
    }

    #[test]
    fn with_the_integration_shows_only_while_the_command_has_text() {
        let mut p = Popup::new("s".into(), size(), None);
        let prompt = b"\x1b]697;StartPrompt\x07$ \x1b]697;EndPrompt\x07\x1b]697;NewCmd=s\x07";
        assert!(!p.on_output(prompt));
        p.on_input(b" ");
        assert!(!p.on_output(b" ")); // only a space
        assert!(p.on_output(b"g"));
        p.on_input(b"\x1b[1;5R");
        assert_eq!(p.last_cursor, Some((4, 0)));
        assert!(!p.on_output(b"\x08\x08\x1b[K")); // erased again: hidden
        assert_eq!(p.last_cursor, None);
    }

    #[test]
    fn enter_deactivates_and_typing_activates() {
        let mut p = Popup::new("s".into(), size(), None);
        p.on_input(b"g");
        assert!(p.typing);
        p.on_input(b"\r");
        assert!(!p.typing);
        p.on_input(b"\x1b[A"); // arrow up browses history, not typing
        assert!(!p.typing);
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
}
