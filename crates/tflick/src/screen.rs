//! Our model of the real screen: alacritty_terminal's grid fed with the shell's output, plus the
//! shell state reported over OSC 697. Reads the command being typed (the edit buffer) from both.
//! port: `get_current_buffer`, `get_text_region`, `new_cmd_internal` in
//! refs/amazon-q-developer-cli-autocomplete/crates/alacritty_terminal/src/term/mod.rs

use alacritty_terminal::Term;
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::vte::ansi::Processor;

use crate::osc::{OscSplitter, Piece};
use crate::shell::{self, Event, Style};

/// The command line being typed, and the cursor in it as a byte index into `text`.
#[derive(Debug, PartialEq)]
pub struct EditBuffer {
    pub text: String,
    pub cursor: usize,
}

/// A grid position that survives scrolling: (line counted from the top of the scrollback,
/// column). Upstream alacritty_terminal has no scroll hook to shift stored points like Q's fork.
type AbsPoint = (i32, usize);

/// Most prompt cells remembered (a long RPROMPT is a few dozen).
const MAX_PROMPT_CELLS: usize = 2000;

#[derive(Default)]
struct ShellState {
    session: String,
    /// Any OSC 697 seen: the shell integration is loaded.
    integrated: bool,
    /// Where the command starts (end of the prompt).
    cmd_start: Option<AbsPoint>,
    /// A command is running (from PreExec to the next NewCmd).
    running: bool,
    prompt_start: Option<AbsPoint>,
    /// Cells drawn by prompts after the command start (RPROMPT, PS2) with their chars, left out
    /// of the buffer.
    /// ponytail: a cell is left out while it still shows the prompt's char, so typing the very
    /// same char over RPROMPT at the same place drops it; per-cell flags (Q's fork) are exact.
    prompt_cells: Vec<(AbsPoint, char)>,
    cwd: Option<String>,
    autosuggest: Option<Style>,
}

pub struct Screen {
    term: Term<VoidListener>,
    parser: Processor,
    osc: OscSplitter,
    shell: ShellState,
}

fn abs(term: &Term<VoidListener>, p: Point) -> AbsPoint {
    (p.line.0 + term.history_size() as i32, p.column.0)
}

fn rel(term: &Term<VoidListener>, (line, col): AbsPoint) -> Point {
    Point::new(Line(line - term.history_size() as i32), Column(col))
}

impl Screen {
    /// `cursor` is where the real terminal's cursor is when we start, (column, row): the screen
    /// usually already has content above (login banner, earlier commands).
    pub fn new(session: String, cols: u16, rows: u16, cursor: Option<(u16, u16)>) -> Self {
        let config = Config {
            // Only scrolled-off parts of a long command are read back; NewCmd clears it anyway.
            scrolling_history: 1000,
            ..Config::default()
        };
        let mut term = Term::new(
            config,
            &TermSize::new(cols.into(), rows.into()),
            VoidListener,
        );
        let mut parser = Processor::new();
        if let Some((col, row)) = cursor {
            // CUP is 1-based.
            parser.advance(
                &mut term,
                format!("\x1b[{};{}H", row + 1, col + 1).as_bytes(),
            );
        }
        Self {
            term,
            parser,
            osc: OscSplitter::default(),
            shell: ShellState {
                session,
                ..ShellState::default()
            },
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.term.resize(TermSize::new(cols.into(), rows.into()));
    }

    /// Shell output, in order. OSC 697 codes are applied with the cursor where they appear.
    pub fn feed(&mut self, bytes: &[u8]) {
        let Self {
            term,
            parser,
            osc,
            shell,
        } = self;
        osc.split(bytes, |piece| match piece {
            Piece::Bytes(b) => parser.advance(term, b),
            Piece::Osc(payload) => shell.apply(shell::parse(&payload), term),
        });
    }

    /// The model's cursor, (column, row). Can drift from the real screen; see `popup`.
    pub fn cursor(&self) -> (u16, u16) {
        let p = self.term.grid().cursor.point;
        (p.column.0 as u16, p.line.0.max(0) as u16)
    }

    /// Full-screen apps (vim, less, htop) use the alternate screen.
    pub fn alt_screen(&self) -> bool {
        self.term.mode().contains(TermMode::ALT_SCREEN)
    }

    pub fn integrated(&self) -> bool {
        self.shell.integrated
    }

    #[allow(dead_code)] // for generators (P5)
    pub fn cwd(&self) -> Option<&str> {
        self.shell.cwd.as_deref()
    }

    /// The command being typed at the prompt; None while a command runs, without the shell
    /// integration, or once the prompt scrolled out of the model.
    pub fn edit_buffer(&self) -> Option<EditBuffer> {
        let (term, s) = (&self.term, &self.shell);
        if !s.integrated || s.running || self.alt_screen() {
            return None;
        }
        let start = rel(term, s.cmd_start?);
        let grid = term.grid();
        let cursor = grid.cursor.point;
        if start.line < term.topmost_line() || cursor < start {
            return None;
        }
        let in_prompt: Vec<Point> = s
            .prompt_cells
            .iter()
            .map(|&(a, c)| (rel(term, a), c))
            .filter(|&(p, c)| p.line >= term.topmost_line() && grid[p].c == c)
            .map(|(p, _)| p)
            .collect();
        let last = term.last_column();

        let mut text = String::new();
        let mut at_cursor = None;
        // Blank cells seen since the last char: spaces if more text follows.
        let mut pad = 0;
        for line in start.line.0..=term.bottommost_line().0 {
            let first = if line == start.line.0 {
                start.column.0
            } else {
                0
            };
            for col in first..=last.0 {
                let p = Point::new(Line(line), Column(col));
                if p == cursor {
                    text.extend(std::iter::repeat_n(' ', pad));
                    pad = 0;
                    at_cursor = Some(text.len());
                }
                let cell = &grid[p];
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue;
                }
                let blank = cell.c == ' '
                    || cell.c == '\0'
                    || in_prompt.contains(&p)
                    || s.autosuggest
                        .as_ref()
                        .is_some_and(|st| st.matches(cell.fg, cell.bg));
                if blank {
                    pad += 1;
                } else {
                    text.extend(std::iter::repeat_n(' ', pad));
                    pad = 0;
                    text.push(cell.c);
                    text.extend(cell.zerowidth().into_iter().flatten());
                }
            }
            // A line the shell ended itself (not wrapped): its trailing blanks are not typed.
            if !grid[Line(line)][last].flags.contains(Flags::WRAPLINE) {
                pad = 0;
            }
        }
        let cursor = at_cursor?;
        text.truncate(text.trim_end().len());
        if text.len() < cursor {
            text.extend(std::iter::repeat_n(' ', cursor - text.len()));
        }
        Some(EditBuffer { text, cursor })
    }
}

impl ShellState {
    fn apply(&mut self, event: Event, term: &mut Term<VoidListener>) {
        self.integrated = true;
        let here = abs(term, term.grid().cursor.point);
        match event {
            Event::StartPrompt => self.prompt_start = Some(here),
            Event::EndPrompt => {
                // Only prompts drawn while typing (RPROMPT, PS2): PS1 comes before NewCmd.
                if let Some(from) = self.prompt_start.take()
                    && self.cmd_start.is_some()
                    && !self.running
                {
                    self.remember_prompt(term, from, here);
                }
            }
            Event::NewCmd(id) => {
                // Codes from a tflick nested deeper (e.g. over ssh) are not about this screen.
                if !id.is_empty() && id != self.session {
                    return;
                }
                // Keeps absolute lines small; this model never shows its scrollback.
                term.grid_mut().clear_history();
                self.cmd_start = Some(abs(term, term.grid().cursor.point));
                self.running = false;
                self.prompt_cells.clear();
            }
            Event::PreExec => self.running = true,
            Event::Dir(dir) => self.cwd = Some(dir),
            Event::AutosuggestStyle(style) => self.autosuggest = Style::parse(&style),
            Event::FishSuggestionColor(color) => self.autosuggest = Style::parse_fish(&color),
            Event::Other => {}
        }
    }

    fn remember_prompt(&mut self, term: &Term<VoidListener>, from: AbsPoint, to: AbsPoint) {
        let cols = term.columns();
        let mut p = from;
        while p < to && self.prompt_cells.len() < MAX_PROMPT_CELLS {
            let at = rel(term, p);
            if at.line >= term.topmost_line() && at.line <= term.bottommost_line() {
                self.prompt_cells.push((p, term.grid()[at].c));
            }
            p = if p.1 + 1 < cols {
                (p.0, p.1 + 1)
            } else {
                (p.0 + 1, 0)
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROMPT: &str = "\x1b]697;StartPrompt\x07$ \x1b]697;EndPrompt\x07\x1b]697;NewCmd=s\x07";

    fn screen(cols: u16, output: &str) -> Screen {
        let mut s = Screen::new("s".into(), cols, 5, None);
        s.feed(output.as_bytes());
        s
    }

    fn buffer(s: &Screen) -> Option<(String, usize)> {
        s.edit_buffer().map(|b| (b.text, b.cursor))
    }

    #[test]
    fn reads_the_typed_command_and_cursor() {
        let s = screen(40, &format!("{PROMPT}git co"));
        assert_eq!(buffer(&s), Some(("git co".into(), 6)));
    }

    #[test]
    fn cursor_moved_back_into_the_command() {
        let s = screen(40, &format!("{PROMPT}git commit -m \"abc\"\x1b[8D"));
        assert_eq!(buffer(&s), Some(("git commit -m \"abc\"".into(), 11)));
    }

    #[test]
    fn trailing_space_before_the_cursor_is_kept() {
        let s = screen(40, &format!("{PROMPT}git "));
        assert_eq!(buffer(&s), Some(("git ".into(), 4)));
    }

    #[test]
    fn wrapped_command_reads_as_one_line_even_after_scrolling() {
        // 10 columns, 5 rows: starts on the last row, the wrap scrolls the screen.
        let s = screen(10, &format!("\n\n\n\n{PROMPT}echo abc defg"));
        assert_eq!(buffer(&s), Some(("echo abc defg".into(), 13)));
    }

    #[test]
    fn grey_autosuggestion_is_left_out() {
        let s = screen(
            40,
            &format!(
                "\x1b]697;ZshAutosuggestionColor=fg=8\x07{PROMPT}git\x1b[38;5;8m status\x1b[39m\x1b[7D"
            ),
        );
        assert_eq!(buffer(&s), Some(("git".into(), 3)));
    }

    #[test]
    fn rprompt_is_left_out() {
        // Like zsh: save cursor, draw RPROMPT at the right edge, restore, then type.
        let rprompt = "\x1b7\x1b[1;30H\x1b]697;StartPrompt\x07[main]\x1b]697;EndPrompt\x07\x1b8";
        let s = screen(40, &format!("{PROMPT}{rprompt}ls -la"));
        assert_eq!(buffer(&s), Some(("ls -la".into(), 6)));
    }

    #[test]
    fn real_zsh_output_with_rprompt() {
        // Captured from `zsh -f` with PS1="%~ %# " RPROMPT="[r]", typing `git co` (80 columns).
        let osc = |p: &str| format!("\x1b]697;{p}\x07");
        let out = format!(
            "\x1b[1m\x1b[7m%\x1b[27m\x1b[1m\x1b[0m{}\r \r{}{}\r\x1b[0m\x1b[27m\x1b[24m\x1b[J\
             {}~/Works/Shizaki/TFlick % {}{}\x1b[K\x1b[51C{}[r]{}\x1b[54D\x1b[?2004hg\x08git co",
            " ".repeat(79),
            osc("Dir=/Users/shizaki/Works/Shizaki/TFlick"),
            osc("ZshAutosuggestionColor="),
            osc("StartPrompt"),
            osc("EndPrompt"),
            osc("NewCmd=s"),
            osc("StartPrompt"),
            osc("EndPrompt"),
        );
        let s = screen(80, &out);
        assert_eq!(buffer(&s), Some(("git co".into(), 6)));
        assert_eq!(s.cwd(), Some("/Users/shizaki/Works/Shizaki/TFlick"));
    }

    #[test]
    fn nothing_while_a_command_runs_or_without_integration() {
        let s = screen(40, &format!("{PROMPT}sleep 10\r\n\x1b]697;PreExec\x07"));
        assert_eq!(buffer(&s), None);
        assert_eq!(buffer(&screen(40, "$ git co")), None);
    }

    #[test]
    fn new_cmd_from_another_session_is_ignored() {
        let s = screen(40, "\x1b]697;NewCmd=other\x07ls");
        assert_eq!(buffer(&s), None);
        assert!(s.integrated());
    }
}
