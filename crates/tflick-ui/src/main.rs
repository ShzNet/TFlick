//! `tflick-ui`: one daemon per user. Listens on a unix socket for `ToUi` messages from every
//! `tflick` and draws the suggestion popup next to the terminal cursor.

mod macos;

use std::borrow::Cow;
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixListener;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use gpui::{
    App, Application, AssetSource, Bounds, Context, FontWeight, SharedString, Window,
    WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div,
    point, prelude::*, px, rgba, size, svg,
};
use macos::{Anchor, Rect};
use tflick_ipc::{Item, Kind, ToUi};

const POPUP_W: f64 = 460.0;
const POPUP_H: f64 = 230.0;

fn icon(kind: Kind) -> &'static str {
    match kind {
        Kind::Subcommand => "icons/subcommand.svg",
        Kind::Option => "icons/option.svg",
        Kind::Arg => "icons/arg.svg",
        Kind::Folder => "icons/folder.svg",
        Kind::File => "icons/file.svg",
        Kind::Branch => "icons/branch.svg",
        Kind::History => "icons/history.svg",
    }
}

/// Accent color 0xRRGGBB00, alpha added by the caller.
fn color(kind: Kind) -> u32 {
    match kind {
        Kind::Subcommand => 0x8b5cf600,
        Kind::Option => 0x3b82f600,
        Kind::Arg => 0x14b8a600,
        Kind::Folder => 0xf59e0b00,
        Kind::File => 0x64748b00,
        Kind::Branch => 0xf9731600,
        Kind::History => 0x22c55e00,
    }
}

/// Icons are embedded in the binary, so the popup works from any working directory.
struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
            "icons/subcommand.svg" => include_bytes!("../icons/subcommand.svg"),
            "icons/option.svg" => include_bytes!("../icons/option.svg"),
            "icons/arg.svg" => include_bytes!("../icons/arg.svg"),
            "icons/folder.svg" => include_bytes!("../icons/folder.svg"),
            "icons/file.svg" => include_bytes!("../icons/file.svg"),
            "icons/branch.svg" => include_bytes!("../icons/branch.svg"),
            "icons/history.svg" => include_bytes!("../icons/history.svg"),
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }

    fn list(&self, _path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

/// Positioning diagnostics on stderr: `TFLICK_DEBUG=1 cargo run -p tflick-ui`.
macro_rules! debug {
    ($($arg:tt)*) => {
        if std::env::var_os("TFLICK_DEBUG").is_some() {
            eprintln!("tflick-ui: {}", format_args!($($arg)*));
        }
    };
}

/// Tuning knobs, overridable without rebuilding: `TFLICK_BLUR=60 TFLICK_TINT=230`.
fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// The cursor cell inside a terminal window, in global top-left coordinates.
///
/// The text grid's origin inside the window is unknown (title bar, tab bar, padding vary per
/// terminal), so we assume the grid is centered horizontally and sits at the bottom with the
/// same padding as the sides: everything left over at the top is title bar + tabs.
/// Only right when the grid fills the window (iTerm2, Terminal.app, Ghostty…), not for terminals
/// embedded in an editor panel; those (xterm.js) give us the real caret instead.
/// ponytail: heuristic; add per-terminal offsets in config when it's off.
fn caret_in_window(
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

/// Where the popup's top-left corner goes: just under the caret, flipped above it near the
/// bottom of the screen, shifted left near the right edge.
fn popup_origin(caret: Rect, screens: &[Rect]) -> (f64, f64) {
    let screen = screens
        .iter()
        .find(|s| caret.x >= s.x && caret.x < s.x + s.w && caret.y >= s.y && caret.y < s.y + s.h)
        .or(screens.first())
        .copied()
        .unwrap_or(caret);
    let mut y = caret.y + caret.h;
    if y + POPUP_H > screen.y + screen.h {
        y = caret.y - POPUP_H;
    }
    let x = caret.x.min(screen.x + screen.w - POPUP_W).max(screen.x);
    (x, y)
}

/// Accepts any number of `tflick` connections; every parsed message goes to the UI thread.
fn listen() -> Receiver<ToUi> {
    let path = tflick_ipc::socket_path();
    // A stale socket from a crashed run would make bind fail.
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).expect("bind tflick-ui socket");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let tx = tx.clone();
            thread::spawn(move || {
                for line in BufReader::new(conn).lines() {
                    let Ok(line) = line else { break };
                    match serde_json::from_str(&line) {
                        Ok(msg) => {
                            if tx.send(msg).is_err() {
                                break;
                            }
                        }
                        Err(e) => eprintln!("tflick-ui: bad message: {e}"),
                    }
                }
            });
        }
    });
    rx
}

struct Popup {
    items: Vec<Item>,
    selected: usize,
    /// Background tint alpha 0-255: higher hides the content behind more.
    tint: u32,
}

impl Popup {
    fn new(window: &mut Window, cx: &mut Context<Self>, rx: Receiver<ToUi>) -> Self {
        let ns_window = macos::ns_window(window).expect("gpui window has an NSWindow");
        macos::set_blur(&ns_window, env_or("TFLICK_BLUR", 30));
        macos::hide(&ns_window);
        cx.spawn_in(window, async move |this, cx| {
            // Session currently shown; Hide from another shell must not close it.
            let mut shown: Option<String> = None;
            loop {
                // ponytail: polls every 16ms; switch to an async channel if idle CPU matters.
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let mut update = None;
                // Window moves happen here, OUTSIDE gpui updates (see macos::show_at).
                for msg in rx.try_iter() {
                    match msg {
                        ToUi::Show {
                            session,
                            items,
                            selected,
                            cursor,
                            grid,
                            cell_px,
                        } => {
                            let anchor = macos::locate_terminal();
                            let caret = match anchor {
                                Some(Anchor::Caret(caret)) => caret,
                                Some(Anchor::Window(win)) => {
                                    caret_in_window(win, cursor, grid, cell_px)
                                }
                                None => {
                                    debug!("no terminal window found (Accessibility allowed?)");
                                    continue;
                                }
                            };
                            let screens = macos::screens();
                            let (x, y) = popup_origin(caret, &screens);
                            debug!(
                                "anchor={anchor:?} cursor={cursor:?} grid={grid:?} \
                                 cell_px={cell_px:?} caret={caret:?} popup=({x}, {y}) \
                                 screens={screens:?}"
                            );
                            macos::show_at(&ns_window, x, y);
                            shown = Some(session);
                            update = Some((items, selected));
                        }
                        ToUi::Hide { session } => {
                            if shown.as_ref() == Some(&session) {
                                macos::hide(&ns_window);
                                shown = None;
                            }
                        }
                    }
                }
                if let Some((items, selected)) = update {
                    let ok = this.update(cx, |this, cx| {
                        this.items = items;
                        this.selected = selected;
                        cx.notify();
                    });
                    if ok.is_err() {
                        break;
                    }
                }
            }
        })
        .detach();
        Self {
            items: Vec::new(),
            selected: 0,
            tint: env_or("TFLICK_TINT", 0xa0u32).min(255),
        }
    }
}

impl Render for Popup {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Follow the system theme like native menus: a light popup with dark text in light mode,
        // so text stays readable over the light apps behind it, and the reverse in dark mode.
        // ponytail: picks colors from the system theme, not from what is actually behind the
        // popup; a dark app under a light-mode popup still relies on the tint for contrast.
        let dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (bg, fg) = if dark {
            (0x1e1e2200, 0xffffff00)
        } else {
            (0xf5f5f700, 0x1d1d1f00)
        };
        let rows = self.items.iter().enumerate().map(|(i, item)| {
            let is_selected = i == self.selected;
            // Bare icon in the kind's accent color (white on the selected row).
            let icon = svg()
                .flex_none()
                .path(icon(item.kind))
                .size_4()
                .text_color(rgba(if is_selected {
                    0xffffffff
                } else {
                    color(item.kind) | 0xff
                }));
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_0p5()
                .rounded_md()
                .when(is_selected, |d| {
                    d.bg(rgba(0x3b82f6ff)).text_color(rgba(0xffffffff))
                })
                .child(icon)
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(SharedString::from(item.name.clone())),
                )
                .children(item.description.clone().map(|desc| {
                    div()
                        .when(!is_selected, |d| d.text_color(rgba(fg | 0xaa)))
                        .truncate()
                        .child(SharedString::from(desc))
                }))
        });
        div()
            .size_full()
            .p_1()
            .rounded_lg()
            .border_1()
            .border_color(rgba(fg | 0x22))
            .bg(rgba(bg | self.tint))
            .text_color(rgba(fg | 0xff))
            .text_sm()
            .font_family(".SystemUIFont")
            .flex()
            .flex_col()
            .children(rows)
    }
}

fn main() {
    let rx = listen();
    Application::new().with_assets(Assets).run(|cx: &mut App| {
        macos::hide_from_dock();
        if !macos::accessibility_trusted(true) {
            eprintln!(
                "tflick-ui: needs Accessibility permission to find the terminal window.\n\
                 Allow it in System Settings > Privacy & Security > Accessibility, then restart."
            );
        }
        let bounds = Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(POPUP_W as f32), px(POPUP_H as f32)),
        };
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                focus: false,
                show: false,
                kind: WindowKind::PopUp,
                is_movable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Popup::new(window, cx, rx)),
        )
        .unwrap();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(popup_origin(caret, &[SCREEN]), (300.0, 216.0));
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
            popup_origin(caret, &[SCREEN]),
            (SCREEN.w - POPUP_W, 900.0 - POPUP_H)
        );
    }
}
