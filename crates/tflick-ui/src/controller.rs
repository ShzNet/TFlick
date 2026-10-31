//! The polling loop: applies messages from `tflick`, finds the terminal through Accessibility,
//! and moves, hides or shows the popup window as [`Follow`] decides.

use std::sync::mpsc::Receiver;
use std::time::Duration;

use core_foundation::base::CFType;
use gpui::{AsyncWindowContext, WeakEntity, px, size};
use objc2::rc::Retained;
use objc2_app_kit::NSWindow;
use tflick_ipc::ToUi;

use crate::follow::{Cmd, Follow, Placed};
use crate::layout::{caret_in_window, popup_origin};
use crate::macos::{self, Anchor};
use crate::settings::Settings;
use crate::settings_view;
use crate::view::Popup;

/// Where the popup should be: the terminal app and the cursor cell it reported.
struct Target {
    pid: i32,
    cursor: (u16, u16),
    grid: (u16, u16),
    cell_px: Option<(f32, f32)>,
}

/// A computed popup position and the terminal window it belongs to.
struct Placement {
    placed: Placed,
    /// The focused AX window, to notice a switch to another window (compared with CFEqual).
    window: CFType,
    /// Placed on the real caret (xterm.js), not inferred from the window frame.
    caret: bool,
}

impl Target {
    /// Looks up the terminal's window now, for a popup of `popup` size. None if it lost focus
    /// or is gone. `typing` wakes Electron's AX tree and logs the result; follow-up polls pass
    /// false.
    fn place(&self, typing: bool, popup: (f64, f64)) -> Option<Placement> {
        let Some(located) = macos::locate_terminal(self.pid, typing) else {
            debug!("no terminal window found (Accessibility allowed?)");
            return None;
        };
        let (caret, is_caret) = match located.anchor {
            Anchor::Caret(caret) => (caret, true),
            Anchor::Window(win) => (
                caret_in_window(win, self.cursor, self.grid, self.cell_px),
                false,
            ),
        };
        let at = popup_origin(caret, &macos::screens(), popup);
        if typing {
            debug!(
                "caret={is_caret} cursor={:?} grid={:?} cell_px={:?} at={caret:?} popup={at:?}",
                self.cursor, self.grid, self.cell_px
            );
        }
        Some(Placement {
            placed: Placed {
                at,
                frame: located.frame,
            },
            window: located.window,
            caret: is_caret,
        })
    }
}

/// What is on screen.
struct Shown {
    /// Hide from another shell must not close it.
    session: String,
    target: Target,
    window: CFType,
    caret: bool,
    follow: Follow,
}

/// Runs for the life of the popup window. Window moves happen here, OUTSIDE gpui updates (see
/// [`macos::show_at`]).
pub async fn run(
    ns_window: Retained<NSWindow>,
    rx: Receiver<ToUi>,
    view: WeakEntity<Popup>,
    cx: &mut AsyncWindowContext,
) {
    let pin = std::env::var_os("TFLICK_PIN").is_some();
    let move_to = |(x, y): (f64, f64)| macos::show_at(&ns_window, x, y);
    let mut shown: Option<Shown> = None;
    // Popup size and blur last applied to the window.
    let mut applied = None;
    loop {
        // ponytail: polls every 16ms; switch to an async channel if idle CPU matters.
        // Faster while the popup follows a drag, so it doesn't trail the window.
        let dragging = shown.as_ref().is_some_and(|s| s.follow.dragging());
        cx.background_executor()
            .timer(Duration::from_millis(if dragging { 4 } else { 16 }))
            .await;

        let Ok(settings) = cx.update(|_, cx| cx.global::<Settings>().clone()) else {
            break;
        };
        let popup = (f64::from(settings.width), f64::from(settings.height));
        let wanted = (settings.width, settings.height, settings.blur);
        if applied != Some(wanted) || (settings.disabled && shown.is_some()) {
            // Changed in the settings window, while the popup is hidden anyway (the terminal
            // lost focus to it).
            macos::hide(&ns_window);
            shown = None;
            if applied != Some(wanted) {
                let _ = cx.update(|window, _| {
                    window.resize(size(px(popup.0 as f32), px(popup.1 as f32)))
                });
                macos::set_blur(&ns_window, settings.blur.into());
                applied = Some(wanted);
            }
        }

        let mut items = None;
        for msg in rx.try_iter() {
            match msg {
                ToUi::OpenSettings => {
                    let _ = cx.update(|_, cx| settings_view::open(cx, None));
                }
                ToUi::Show { .. } if settings.disabled => {}
                ToUi::Show {
                    session,
                    items: new_items,
                    selected,
                    cursor,
                    grid,
                    cell_px,
                } => {
                    let Some(pid) = macos::focused_app_pid() else {
                        continue;
                    };
                    let target = Target {
                        pid,
                        cursor,
                        grid,
                        cell_px,
                    };
                    let Some(p) = target.place(true, popup) else {
                        continue;
                    };
                    move_to(p.placed.at);
                    shown = Some(Shown {
                        session,
                        target,
                        window: p.window,
                        caret: p.caret,
                        follow: Follow::new(p.placed),
                    });
                    items = Some((new_items, selected));
                }
                ToUi::Hide { session } => {
                    if shown.as_ref().is_some_and(|s| s.session == session) {
                        macos::hide(&ns_window);
                        shown = None;
                    }
                }
            }
        }

        if let Some(s) = &mut shown {
            let (down, mouse) = (macos::left_mouse_down(), macos::mouse_location());
            if let Some(at) = s.follow.drag(down, mouse) {
                // Mid-drag only the mouse matters: skip the slow AX lookups.
                move_to(at);
            } else {
                let now = (macos::focused_app_pid() == Some(s.target.pid))
                    .then(|| s.target.place(false, popup))
                    .flatten();
                match now {
                    // Same window, and still on the terminal's caret if we had one (clicking
                    // from a VSCode terminal into the editor loses it).
                    Some(p) if p.window == s.window && p.caret == s.caret => {
                        match s.follow.tick(p.placed, down, mouse) {
                            Some(Cmd::Hide) => macos::hide(&ns_window),
                            Some(Cmd::MoveTo(at)) => move_to(at),
                            None => {}
                        }
                    }
                    // Focus left: another app, another window, the terminal quit or closed.
                    // TFLICK_PIN=1 keeps it, e.g. to take a screenshot.
                    _ if pin => {}
                    _ => {
                        macos::hide(&ns_window);
                        shown = None;
                    }
                }
            }
        }

        if let Some((items, selected)) = items {
            let ok = view.update(cx, |view, cx| {
                view.items = items;
                view.selected = selected;
                cx.notify();
            });
            if ok.is_err() {
                break;
            }
        }
    }
}
