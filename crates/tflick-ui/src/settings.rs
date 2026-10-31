//! User settings in `~/.config/tflick/settings.json`, with flat keys like Amazon Q's
//! (`autocomplete.theme`). Missing keys take their default, so the file only needs what changed.
//! Held as a gpui global: views re-render when it changes.

use std::path::PathBuf;
use std::{env, fs, io};

use gpui::{App, BorrowAppContext, Global};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Follow macOS light/dark mode.
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(rename = "autocomplete.disable")]
    pub disabled: bool,
    #[serde(rename = "autocomplete.theme")]
    pub theme: Theme,
    /// Popup size, in points.
    #[serde(rename = "autocomplete.width")]
    pub width: u32,
    #[serde(rename = "autocomplete.height")]
    pub height: u32,
    #[serde(rename = "autocomplete.fontSize")]
    pub font_size: u32,
    /// Blur radius of what is behind the popup.
    #[serde(rename = "autocomplete.blur")]
    pub blur: u32,
    /// Background opacity 0-255: higher hides what is behind more.
    #[serde(rename = "autocomplete.tint")]
    pub tint: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            disabled: false,
            theme: Theme::System,
            width: 460,
            height: 230,
            font_size: 14,
            blur: 30,
            tint: 160,
        }
    }
}

impl Global for Settings {}

/// Allowed values of the numeric settings: (min, max, step), as the settings window steps them.
pub const WIDTH: (u32, u32, u32) = (300, 800, 20);
pub const HEIGHT: (u32, u32, u32) = (120, 500, 10);
pub const FONT_SIZE: (u32, u32, u32) = (11, 20, 1);
pub const BLUR: (u32, u32, u32) = (0, 80, 5);
pub const TINT: (u32, u32, u32) = (0, 255, 15);

pub fn path() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".config"))
        .join("tflick/settings.json")
}

impl Settings {
    /// The saved settings; defaults when there is no file, and (logged) when it is broken.
    pub fn load() -> Self {
        let path = path();
        match fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text).unwrap_or_else(|e| {
                eprintln!("tflick-ui: {}: {e}; using defaults", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    fn parse(text: &str) -> serde_json::Result<Self> {
        let mut s: Self = serde_json::from_str(text)?;
        // Hand-edited values outside the range would break the layout.
        for (v, (min, max, _)) in [
            (&mut s.width, WIDTH),
            (&mut s.height, HEIGHT),
            (&mut s.font_size, FONT_SIZE),
            (&mut s.blur, BLUR),
            (&mut s.tint, TINT),
        ] {
            *v = (*v).clamp(min, max);
        }
        Ok(s)
    }

    pub fn save(&self) -> io::Result<()> {
        let path = path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, serde_json::to_string_pretty(self)? + "\n")
    }

    /// Changes the global settings and saves them; every view showing them re-renders.
    pub fn change(cx: &mut App, f: impl FnOnce(&mut Self)) {
        cx.update_global::<Self, _>(|s, _| f(s));
        if let Err(e) = cx.global::<Self>().save() {
            eprintln!("tflick-ui: saving {}: {e}", path().display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_keys_default_and_out_of_range_values_are_clamped() {
        let s =
            Settings::parse(r#"{"autocomplete.theme": "dark", "autocomplete.width": 5}"#).unwrap();
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.width, WIDTH.0);
        assert_eq!(s.height, Settings::default().height);
    }

    #[test]
    fn round_trips_with_q_style_keys() {
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert!(json.contains(r#""autocomplete.fontSize":14"#));
        assert_eq!(Settings::parse(&json).unwrap(), Settings::default());
    }
}
