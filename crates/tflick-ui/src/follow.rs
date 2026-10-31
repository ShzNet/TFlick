//! When to move, hide or reshow the popup while the terminal window moves. Pure decisions over
//! polled observations (window frame, mouse); the controller makes the OS calls.

use crate::layout::Rect;

pub type Point = (f64, f64);

/// Where the popup is placed, and the terminal window frame it was placed from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub at: Point,
    pub frame: Rect,
}

#[derive(Debug, PartialEq)]
pub enum Cmd {
    Hide,
    MoveTo(Point),
}

/// Polls a moved window must stay put, with the mouse released, before the popup is placed from
/// AX again. AX reports a dragged window's position in bursts, with gaps longer than a few polls,
/// so the mouse button is what really tells a drag is over.
const SETTLE_POLLS: u32 = 6;

/// Height of a standard macOS title bar, in points.
const TITLE_BAR_H: f64 = 28.0;

pub struct Follow {
    placed: Placed,
    /// Polls the window frame has stayed the same.
    still: u32,
    /// Mouse position and popup position when the left button went down on a settled popup.
    press: Option<(Point, Point)>,
    /// The window moves with that press (title bar drag): the popup follows the mouse.
    follow: bool,
}

impl Follow {
    /// A popup just shown at `placed`, settled.
    pub fn new(placed: Placed) -> Self {
        Self {
            placed,
            still: SETTLE_POLLS,
            press: None,
            follow: false,
        }
    }

    /// Following a title bar drag with the mouse held: poll faster, and skip AX via [`Self::drag`].
    pub fn dragging(&self) -> bool {
        self.follow && self.press.is_some()
    }

    /// Mid-drag popup position from the mouse alone: a title bar drag moves the window exactly
    /// with the mouse, and the mouse position is live while AX frames lag in bursts.
    pub fn drag(&self, mouse_down: bool, mouse: Point) -> Option<Point> {
        let (m0, at0) = self.press.filter(|_| self.follow && mouse_down)?;
        Some((at0.0 + mouse.0 - m0.0, at0.1 + mouse.1 - m0.1))
    }

    /// One poll with the terminal window's current placement (same window as before).
    pub fn tick(&mut self, now: Placed, mouse_down: bool, mouse: Point) -> Option<Cmd> {
        if !mouse_down {
            self.press = None;
        } else if self.press.is_none() && self.still >= SETTLE_POLLS {
            self.press = Some((mouse, self.placed.at));
            // Pressed on the title bar: follow from the first mouse move instead of waiting for
            // AX's late first frame report.
            // ponytail: fixed title bar height; a tab dragged out of the bar is followed by
            // mistake until release corrects it.
            let bar = Rect {
                h: TITLE_BAR_H,
                ..now.frame
            };
            if bar.contains(mouse) {
                self.follow = true;
                self.still = 0;
            }
        }
        let mut cmd = None;
        if now.frame != self.placed.frame {
            // Resizes and keyboard moves (Rectangle…) can't be predicted: hide until settled.
            // Keep following after release: AX still reports a few late frames then.
            let resized = (now.frame.w, now.frame.h) != (self.placed.frame.w, self.placed.frame.h);
            self.follow = (self.follow || self.press.is_some()) && !resized;
            if !self.follow {
                cmd = Some(Cmd::Hide);
            }
            self.still = 0;
        }
        if let Some(at) = self.drag(mouse_down, mouse) {
            cmd = Some(Cmd::MoveTo(at));
        } else if now.frame == self.placed.frame && self.still < SETTLE_POLLS {
            // Mouse held restarts the wait.
            self.still = if mouse_down { 0 } else { self.still + 1 };
            if self.still == SETTLE_POLLS {
                // Also corrects the mouse-predicted position (the window may have been stopped
                // at the menu bar).
                cmd = Some(Cmd::MoveTo(now.at));
                self.follow = false;
            }
        } else if self.still >= SETTLE_POLLS && now.at != self.placed.at {
            // Same window, caret moved (xterm.js caret while typing).
            cmd = Some(Cmd::MoveTo(now.at));
        }
        self.placed = now;
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placed(x: f64, y: f64, w: f64) -> Placed {
        Placed {
            at: (x + 100.0, y + 200.0),
            frame: Rect { x, y, w, h: 600.0 },
        }
    }

    /// Ticks with the mouse up until the popup is placed again.
    fn settle(f: &mut Follow, now: Placed) -> Option<Cmd> {
        (0..SETTLE_POLLS)
            .filter_map(|_| f.tick(now, false, (0.0, 0.0)))
            .last()
    }

    #[test]
    fn title_bar_drag_follows_the_mouse_then_settles_from_ax() {
        let start = placed(0.0, 0.0, 800.0);
        let mut f = Follow::new(start);
        // Press on the title bar: following starts before AX reports any move.
        assert_eq!(
            f.tick(start, true, (50.0, 10.0)),
            Some(Cmd::MoveTo(start.at))
        );
        assert!(f.dragging());
        assert_eq!(f.drag(true, (80.0, 30.0)), Some((130.0, 220.0)));
        // Released; AX catches up with late frames, the popup stays where the mouse put it.
        let end = placed(30.0, 20.0, 800.0);
        assert_eq!(f.tick(end, false, (80.0, 30.0)), None);
        assert!(!f.dragging());
        assert_eq!(settle(&mut f, end), Some(Cmd::MoveTo(end.at)));
    }

    #[test]
    fn a_click_in_the_content_does_not_follow() {
        let start = placed(0.0, 0.0, 800.0);
        let mut f = Follow::new(start);
        assert_eq!(f.tick(start, true, (50.0, 300.0)), None);
        assert_eq!(f.drag(true, (90.0, 340.0)), None);
    }

    #[test]
    fn resize_hides_until_settled() {
        let mut f = Follow::new(placed(0.0, 0.0, 800.0));
        let wider = placed(0.0, 0.0, 900.0);
        assert_eq!(f.tick(wider, false, (0.0, 0.0)), Some(Cmd::Hide));
        assert_eq!(settle(&mut f, wider), Some(Cmd::MoveTo(wider.at)));
    }

    #[test]
    fn caret_moving_in_the_same_window_moves_the_popup() {
        let start = placed(0.0, 0.0, 800.0);
        let mut f = Follow::new(start);
        let typed = Placed {
            at: (start.at.0 + 8.0, start.at.1),
            ..start
        };
        assert_eq!(
            f.tick(typed, false, (0.0, 0.0)),
            Some(Cmd::MoveTo(typed.at))
        );
    }
}
