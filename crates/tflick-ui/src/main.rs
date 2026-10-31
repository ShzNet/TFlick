//! P2a spike: a non-activating blurred gpui popup that follows the mouse every 50ms,
//! changes its content and toggles visibility, to measure flicker, CPU and latency.
//! `cargo run -p tflick-ui` then watch; click the popup and check the app behind keeps focus.

use std::time::{Duration, Instant};

use gpui::{
    App, Application, Bounds, Context, FontWeight, SharedString, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, point, prelude::*,
    px, rgba, size,
};
use objc2::rc::Retained;
use objc2_app_kit::{NSEvent, NSView, NSWindow};
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

/// The NSWindow behind a gpui window.
/// ponytail: gpui 0.2.2 has no public move API, so we reach the NSWindow through the raw
/// handle. Drop this once gpui exposes `set_position`.
fn ns_window(window: &Window) -> Option<Retained<NSWindow>> {
    // gpui::Window has an inherent `window_handle()` too, so call the trait explicitly.
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(h) = handle.as_raw() else {
        return None;
    };
    // SAFETY: gpui hands out a valid NSView pointer for the lifetime of the window.
    let view: &NSView = unsafe { h.ns_view.cast().as_ref() };
    view.window()
}

/// Moves the popup just below-right of the mouse and shows or hides it.
///
/// Must run OUTSIDE any gpui `update`: moving to a screen with another scale makes AppKit call
/// gpui's resize callback synchronously, and that callback fails while the window is borrowed
/// by an update, leaving gpui's scale factor stale (content drawn at half or double size).
fn follow_mouse(ns_window: &NSWindow, visible: bool) {
    // Both values are in Cocoa coordinates (origin bottom-left), so no flipping needed here.
    let mouse = NSEvent::mouseLocation();
    ns_window.setFrameTopLeftPoint(NSPoint::new(mouse.x + 12.0, mouse.y - 18.0));
    if !visible {
        ns_window.orderOut(None);
    } else if !ns_window.isVisible() {
        ns_window.orderFrontRegardless();
    }
}

impl Popup {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let ns_window = ns_window(window).expect("gpui window has an NSWindow");
        cx.spawn_in(window, async move |this, cx| {
            let mut tick = 0usize;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                tick += 1;
                let start = Instant::now();
                // This async task runs on the main thread, between gpui updates.
                // Visible 3s, hidden 0.5s, to check for flicker on re-show.
                follow_mouse(&ns_window, tick % 70 < 60);
                let ok = this.update_in(cx, |this, window, cx| {
                    this.tick = tick;
                    cx.notify();
                    this.max_frame = this.max_frame.max(start.elapsed());
                    // Log scale changes (moving between Retina and 1x screens).
                    let sf = window.scale_factor();
                    if sf != this.last_scale {
                        eprintln!("tick {tick} scale {sf} bounds {:?}", window.bounds().origin);
                        this.last_scale = sf;
                    }
                    if tick % 100 == 0 {
                        eprintln!("tick {tick} max update {:?}", this.max_frame);
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
