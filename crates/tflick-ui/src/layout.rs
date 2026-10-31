//! Pure geometry: where the popup goes, given the terminal window or caret. No OS calls.

/// A rectangle in global top-left coordinates (AX / gpui space), in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn contains(&self, (x, y): (f64, f64)) -> bool {
        (self.x..self.x + self.w).contains(&x) && (self.y..self.y + self.h).contains(&y)
    }
}

/// The cursor cell inside a terminal window, in global top-left coordinates.
///
/// The text grid's origin inside the window is unknown (title bar, tab bar, padding vary per
/// terminal), so we assume the grid is centered horizontally and sits at the bottom with the
/// same padding as the sides: everything left over at the top is title bar + tabs.
/// Only right when the grid fills the window (iTerm2, Terminal.app, Ghostty…), not for terminals
/// embedded in an editor panel; those (xterm.js) give us the real caret instead.
/// ponytail: heuristic; add per-terminal offsets in config when it's off.
pub fn caret_in_window(
    win: Rect,
    cursor: (u16, u16),
    grid: (u16, u16),
    cell_px: Option<(f32, f32)>,
) -> Rect {
    let (cols, rows) = (f64::from(grid.0.max(1)), f64::from(grid.1.max(1)));
    let (cw, ch) = match cell_px {
        Some((w, h)) => {
            let (w, h) = (f64::from(w), f64::from(h));
            // Some terminals report device pixels, not points: the grid would be ~2x the window.
            let scale = (w * cols / win.w).round().max(1.0);
            (w / scale, h / scale)
        }
        None => (win.w / cols, (win.h - 28.0) / rows),
    };
    let side = ((win.w - cols * cw) / 2.0).max(0.0);
    let top = (win.h - rows * ch - side).max(0.0);
    Rect {
        x: win.x + side + f64::from(cursor.0) * cw,
        y: win.y + top + f64::from(cursor.1) * ch,
        w: cw,
        h: ch,
    }
}

/// Where a popup of `(w, h)` puts its top-left corner: just under the caret, flipped above it
/// near the bottom of the screen, shifted left near the right edge.
pub fn popup_origin(caret: Rect, screens: &[Rect], (w, h): (f64, f64)) -> (f64, f64) {
    let screen = screens
        .iter()
        .find(|s| s.contains((caret.x, caret.y)))
        .or(screens.first())
        .copied()
        .unwrap_or(caret);
    let mut y = caret.y + caret.h;
    if y + h > screen.y + screen.h {
        y = caret.y - h;
    }
    let x = caret.x.min(screen.x + screen.w - w).max(screen.x);
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const POPUP: (f64, f64) = (460.0, 230.0);
    const SCREEN: Rect = Rect {
        x: 0.0,
        y: 25.0,
        w: 1512.0,
        h: 920.0,
    };

    #[test]
    fn caret_with_title_bar_inferred() {
        // 80x24 grid of 8x16pt cells, 4pt side padding, 28pt title bar.
        let win = Rect {
            x: 100.0,
            y: 100.0,
            w: 648.0,
            h: 28.0 + 384.0 + 4.0,
        };
        let c = caret_in_window(win, (10, 2), (80, 24), Some((8.0, 16.0)));
        assert_eq!(
            c,
            Rect {
                x: 100.0 + 4.0 + 80.0,
                y: 100.0 + 28.0 + 32.0,
                w: 8.0,
                h: 16.0
            }
        );
    }

    #[test]
    fn device_pixel_cell_sizes_are_scaled_down() {
        let win = Rect {
            x: 100.0,
            y: 100.0,
            w: 648.0,
            h: 416.0,
        };
        let points = caret_in_window(win, (10, 2), (80, 24), Some((8.0, 16.0)));
        let pixels = caret_in_window(win, (10, 2), (80, 24), Some((16.0, 32.0)));
        assert_eq!(points, pixels);
    }

    #[test]
    fn popup_goes_below_caret() {
        let caret = Rect {
            x: 300.0,
            y: 200.0,
            w: 8.0,
            h: 16.0,
        };
        assert_eq!(popup_origin(caret, &[SCREEN], POPUP), (300.0, 216.0));
    }

    #[test]
    fn flips_above_near_screen_bottom_and_shifts_left_near_right_edge() {
        let caret = Rect {
            x: 1400.0,
            y: 900.0,
            w: 8.0,
            h: 16.0,
        };
        assert_eq!(
            popup_origin(caret, &[SCREEN], POPUP),
            (SCREEN.w - POPUP.0, 900.0 - POPUP.1)
        );
    }
}
