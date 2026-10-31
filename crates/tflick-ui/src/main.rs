//! P2a spike: a non-activating blurred gpui popup that follows the mouse every 50ms,
//! changes its content and toggles visibility, to measure flicker, CPU and latency.
//! `cargo run -p tflick-ui` then watch; click the popup and check the app behind keeps focus.

use std::time::{Duration, Instant};

use gpui::{
    App, Application, Bounds, Context, FontWeight, SharedString, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, point, prelude::*,
    px, rgba, size,
};
use objc2_app_kit::{NSEvent, NSView};
use objc2_foundation::NSPoint;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

const ITEMS: [(&str, &str); 8] = [
    ("checkout", "Switch branches or restore working tree files"),
    (
        "cherry-pick",
        "Apply the changes introduced by some existing commits",
    ),
    ("clean", "Remove untracked files from the working tree"),
    ("clone", "Clone a repository into a new directory"),
    ("commit", "Record changes to the repository"),
    ("config", "Get and set repository or global options"),
    (
        "diff",
        "Show changes between commits, commit and working tree",
    ),
    ("fetch", "Download objects and refs from another repository"),
];

struct Popup {
    tick: usize,
    max_frame: Duration,
    last_scale: f32,
}

/// Moves the window so its top-left corner sits just below-right of the mouse, and shows or
/// hides it (orderOut/orderFront, like the real popup will on Show/Hide).
/// ponytail: gpui 0.2.2 has no public move API, so we reach the NSWindow through the raw
/// handle. Drop this once gpui exposes `set_position`.
fn follow_mouse(window: &Window, visible: bool) {
    // gpui::Window has an inherent `window_handle()` too, so call the trait explicitly.
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(h) = handle.as_raw() else {
        return;
    };
    // SAFETY: gpui hands out a valid NSView pointer for the lifetime of the window, and we are
    // on the main thread (gpui runs all window callbacks there).
    unsafe {
        let view: &NSView = h.ns_view.cast().as_ref();
        let Some(ns_window) = view.window() else {
            return;
        };
        // Both values are in Cocoa coordinates (origin bottom-left), so no flipping needed here.
        let mouse = NSEvent::mouseLocation();
        let old_scale = ns_window.backingScaleFactor();
        ns_window.setFrameTopLeftPoint(NSPoint::new(mouse.x + 12.0, mouse.y - 18.0));
        // Moving to a screen with another scale (Retina 2x <-> external 1x): AppKit tells gpui
        // a beat late, so the next frame would render at the old scale (text 2x too big).
        // Notify gpui right away so it updates its scale before drawing.
        if ns_window.backingScaleFactor() != old_scale {
            view.viewDidChangeBackingProperties();
        }
        if !visible {
            ns_window.orderOut(None);
        } else if !ns_window.isVisible() {
            ns_window.orderFrontRegardless();
        }
    }
}

impl Popup {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                let ok = this.update_in(cx, |this, window, cx| {
                    let start = Instant::now();
                    this.tick += 1;
                    // Visible 3s, hidden 0.5s, to check for flicker on re-show.
                    follow_mouse(window, this.tick % 70 < 60);
                    cx.notify();
                    this.max_frame = this.max_frame.max(start.elapsed());
                    // Log scale changes (moving between Retina and 1x screens).
                    let sf = window.scale_factor();
                    if sf != this.last_scale {
                        eprintln!(
                            "tick {} scale {sf} bounds {:?}",
                            this.tick,
                            window.bounds().origin
                        );
                        this.last_scale = sf;
                    }
                    if this.tick % 100 == 0 {
                        eprintln!("tick {} max update {:?}", this.tick, this.max_frame);
                        this.max_frame = Duration::ZERO;
                    }
                });
                if ok.is_err() {
                    break;
                }
            }
        })
        .detach();
        Self {
            tick: 0,
            max_frame: Duration::ZERO,
            last_scale: 0.0,
        }
    }
}

impl Render for Popup {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Rotate items so content changes every tick, like a real popup while typing.
        let selected = (self.tick / 10) % ITEMS.len();
        let rows = (0..ITEMS.len()).map(|i| {
            let (name, desc) = ITEMS[(i + self.tick / 20) % ITEMS.len()];
            div()
                .flex()
                .gap_3()
                .px_2()
                .py_0p5()
                .rounded_md()
                .when(i == selected, |d| d.bg(rgba(0x3b82f6cc)))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(SharedString::from(name)),
                )
                .child(
                    div()
                        .text_color(rgba(0xffffff99))
                        .truncate()
                        .child(SharedString::from(desc)),
                )
        });
        div()
            .size_full()
            .p_1()
            .rounded_lg()
            .border_1()
            .border_color(rgba(0xffffff22))
            .bg(rgba(0x1e1e2299))
            .text_color(rgba(0xffffffee))
            .text_sm()
            .font_family(".SystemUIFont")
            .flex()
            .flex_col()
            .children(rows)
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds {
            origin: point(px(200.), px(200.)),
            size: size(px(460.), px(230.)),
        };
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                focus: false,
                show: true,
                kind: WindowKind::PopUp,
                is_movable: false,
                window_background: WindowBackgroundAppearance::Blurred,
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Popup::new(window, cx)),
        )
        .unwrap();
    });
}
