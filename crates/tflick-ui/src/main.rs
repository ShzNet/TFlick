//! P2a spike: a non-activating blurred gpui popup that follows the mouse every 50ms,
//! changes its content and toggles visibility, to measure flicker, CPU and latency.
//! `cargo run -p tflick-ui` then watch; click the popup and check the app behind keeps focus.

use std::borrow::Cow;
use std::time::{Duration, Instant};

use gpui::{
    App, Application, AssetSource, Bounds, Context, FontWeight, SharedString, Window,
    WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div,
    point, prelude::*, px, rgba, size, svg,
};
use objc2::rc::Retained;
use objc2_app_kit::{NSEvent, NSView, NSWindow};
use objc2_foundation::NSPoint;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Suggestion kinds, matching Fig's suggestion types; each has an icon and an accent color.
#[derive(Clone, Copy)]
enum Kind {
    Subcommand,
    Option,
    Arg,
    Folder,
    File,
    Branch,
    History,
}

impl Kind {
    fn icon(self) -> &'static str {
        match self {
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
    fn color(self) -> u32 {
        match self {
            Kind::Subcommand => 0x8b5cf600,
            Kind::Option => 0x3b82f600,
            Kind::Arg => 0x14b8a600,
            Kind::Folder => 0xf59e0b00,
            Kind::File => 0x64748b00,
            Kind::Branch => 0xf9731600,
            Kind::History => 0x22c55e00,
        }
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

const ITEMS: [(Kind, &str, &str); 8] = [
    (
        Kind::Subcommand,
        "checkout",
        "Switch branches or restore working tree files",
    ),
    (
        Kind::Subcommand,
        "commit",
        "Record changes to the repository",
    ),
    (
        Kind::Option,
        "--amend",
        "Replace the tip of the current branch",
    ),
    (
        Kind::Arg,
        "<pathspec>",
        "Limit the paths affected by the operation",
    ),
    (Kind::Folder, "src/", "Folder"),
    (Kind::File, "Cargo.toml", "File"),
    (Kind::Branch, "main", "Branch · 2 hours ago"),
    (Kind::History, "git commit -m \"wip\"", "From history"),
];

struct Popup {
    tick: usize,
    /// Background tint alpha 0-255: higher hides the content behind more.
    tint: u32,
    max_frame: Duration,
    last_scale: f32,
}

// SkyLight private API, same declaration gpui uses (gpui-0.2.2/src/platform/mac/window.rs:94).
// Also what Terminal.app, kitty and Alacritty use for background blur.
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGSMainConnectionID() -> *mut std::ffi::c_void;
    fn CGSSetWindowBackgroundBlurRadius(
        connection_id: *mut std::ffi::c_void,
        window_id: isize,
        radius: i64,
    ) -> i32;
}

/// Tuning knobs, overridable without rebuilding: `TFLICK_BLUR=60 TFLICK_TINT=230`.
fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Blurs whatever is behind the window. gpui's `Blurred` uses a light NSVisualEffectView with no
/// radius control, so we keep the window `Transparent` and set the blur radius ourselves.
fn set_blur(ns_window: &NSWindow, radius: i64) {
    // SAFETY: plain C call on our own window number; returns an error code we can ignore.
    unsafe {
        CGSSetWindowBackgroundBlurRadius(CGSMainConnectionID(), ns_window.windowNumber(), radius);
    }
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
        set_blur(&ns_window, env_or("TFLICK_BLUR", 30));
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
            tint: env_or("TFLICK_TINT", 0xa0u32).min(255),
            max_frame: Duration::ZERO,
            last_scale: 0.0,
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
        // Rotate items so content changes every tick, like a real popup while typing.
        let selected = (self.tick / 10) % ITEMS.len();
        let rows = (0..ITEMS.len()).map(|i| {
            let (kind, name, desc) = ITEMS[(i + self.tick / 20) % ITEMS.len()];
            let is_selected = i == selected;
            // Bare icon in the kind's accent color (white on the selected row).
            let icon = svg()
                .flex_none()
                .path(kind.icon())
                .size_4()
                .text_color(rgba(if is_selected {
                    0xffffffff
                } else {
                    kind.color() | 0xff
                }));
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_0p5()
                .rounded_md()
                .when(i == selected, |d| {
                    d.bg(rgba(0x3b82f6ff)).text_color(rgba(0xffffffff))
                })
                .child(icon)
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(SharedString::from(name)),
                )
                .child(
                    div()
                        .when(i != selected, |d| d.text_color(rgba(fg | 0xaa)))
                        .truncate()
                        .child(SharedString::from(desc)),
                )
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
    Application::new().with_assets(Assets).run(|cx: &mut App| {
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
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Popup::new(window, cx)),
        )
        .unwrap();
    });
}
