//! Messages between `tflick` (one per shell) and `tflick-ui` (one per user), sent as one JSON
//! object per line over a unix socket.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// ponytail: relies on macOS `$TMPDIR` being per-user (mode 0700); use a uid-scoped dir with
/// explicit 0600 permissions when Linux support lands.
pub fn socket_path() -> PathBuf {
    std::env::temp_dir().join("tflick-ui.sock")
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToUi {
    Show {
        session: String,
        items: Vec<Item>,
        selected: usize,
        /// Cursor cell (column, row), row 0 at the top of the visible screen.
        cursor: (u16, u16),
        /// Terminal size in cells (columns, rows).
        grid: (u16, u16),
        /// Size of one cell in points, when the terminal reports pixel sizes (TIOCGWINSZ).
        cell_px: Option<(f32, f32)>,
    },
    Hide {
        session: String,
    },
    /// `tflick settings`: open (or bring to front) the settings window.
    OpenSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub name: String,
    pub description: Option<String>,
    pub kind: Kind,
}

/// Suggestion kinds, matching Fig's suggestion types.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Kind {
    Subcommand,
    Option,
    Arg,
    Folder,
    File,
    Branch,
    History,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_roundtrips_as_one_json_line() {
        let msg = ToUi::Show {
            session: "s".into(),
            items: vec![Item {
                name: "commit".into(),
                description: None,
                kind: Kind::Subcommand,
            }],
            selected: 0,
            cursor: (4, 2),
            grid: (80, 24),
            cell_px: Some((7.5, 16.0)),
        };
        let line = serde_json::to_string(&msg).unwrap();
        assert!(!line.contains('\n'));
        let ToUi::Show { cursor, items, .. } = serde_json::from_str(&line).unwrap() else {
            panic!("wrong variant");
        };
        assert_eq!(cursor, (4, 2));
        assert_eq!(items[0].name, "commit");
    }
}
