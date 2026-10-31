//! What the shell integration reports over OSC 697 (see `shell/tflick.zsh`).
//! port: refs/amazon-q-developer-cli-autocomplete/crates/alacritty_terminal/src/ansi.rs (OSC 697)

use alacritty_terminal::vte::ansi::{Color, Rgb};

#[derive(Debug, PartialEq)]
pub enum Event {
    /// A prompt (PS1, PS2, RPROMPT) starts drawing at the cursor.
    StartPrompt,
    /// The prompt ends at the cursor.
    EndPrompt,
    /// The user's command starts at the cursor. Carries the session id of the tflick it is for.
    NewCmd(String),
    /// The command is about to run.
    PreExec,
    Dir(String),
    /// `ZSH_AUTOSUGGEST_HIGHLIGHT_STYLE`, e.g. `fg=8`.
    AutosuggestStyle(String),
    /// Known but unused for now (Shell, PID, ExitCode…), or unknown.
    Other,
}

pub fn parse(payload: &str) -> Event {
    match payload.split_once('=') {
        None => match payload {
            "StartPrompt" => Event::StartPrompt,
            "EndPrompt" => Event::EndPrompt,
            "PreExec" => Event::PreExec,
            _ => Event::Other,
        },
        Some(("NewCmd", id)) => Event::NewCmd(id.into()),
        Some(("Dir", dir)) => Event::Dir(dir.into()),
        Some(("ZshAutosuggestionColor", style)) => Event::AutosuggestStyle(style.into()),
        Some(_) => Event::Other,
    }
}

/// Colors of zsh-autosuggestions' grey suggestion text, which is not part of the command.
#[derive(Debug, Default, PartialEq)]
pub struct Style {
    fg: Option<Color>,
    bg: Option<Color>,
}

impl Style {
    /// zsh highlight syntax: `fg=8`, `fg=#808080,bg=black,underline`. None when no colors (the
    /// plugin isn't loaded, or the style can't be told apart from typed text).
    pub fn parse(s: &str) -> Option<Self> {
        let mut style = Self::default();
        for part in s.split(',') {
            match part.trim().split_once('=') {
                Some(("fg", c)) => style.fg = parse_color(c),
                Some(("bg", c)) => style.bg = parse_color(c),
                _ => {}
            }
        }
        (style != Self::default()).then_some(style)
    }

    pub fn matches(&self, fg: Color, bg: Color) -> bool {
        let same = |want: Option<Color>, have| want.is_none_or(|w| normalize(w) == normalize(have));
        same(self.fg, fg) && same(self.bg, bg)
    }
}

fn parse_color(s: &str) -> Option<Color> {
    const NAMES: [&str; 8] = [
        "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
    ];
    if let Some(hex) = s.strip_prefix('#') {
        let v = u32::from_str_radix(hex, 16)
            .ok()
            .filter(|_| hex.len() == 6)?;
        return Some(Color::Spec(Rgb {
            r: (v >> 16) as u8,
            g: (v >> 8) as u8,
            b: v as u8,
        }));
    }
    let index = s
        .parse()
        .ok()
        .or_else(|| NAMES.iter().position(|&n| n == s).map(|i| i as u8))?;
    Some(Color::Indexed(index))
}

/// The 16 base colors arrive either as SGR 30-37/90-97 (Named) or 38;5;n (Indexed).
fn normalize(c: Color) -> Color {
    match c {
        Color::Named(n) if (n as usize) < 16 => Color::Indexed(n as u8),
        c => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::NamedColor;

    #[test]
    fn parses_events() {
        assert_eq!(parse("NewCmd=abc"), Event::NewCmd("abc".into()));
        assert_eq!(parse("Dir=/a=b"), Event::Dir("/a=b".into()));
        assert_eq!(parse("EndPrompt"), Event::EndPrompt);
        assert_eq!(parse("PID=12"), Event::Other);
    }

    #[test]
    fn autosuggest_style_matches_named_and_indexed_colors() {
        let s = Style::parse("fg=8,underline").unwrap();
        let bg = Color::Named(NamedColor::Background);
        assert!(s.matches(Color::Indexed(8), bg));
        assert!(s.matches(Color::Named(NamedColor::BrightBlack), bg));
        assert!(!s.matches(Color::Named(NamedColor::Foreground), bg));
        assert!(Style::parse("fg=#ff0080").is_some());
        assert_eq!(Style::parse(""), None);
    }
}
