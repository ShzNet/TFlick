//! `tflick-ui`: one daemon per user. Listens on a unix socket for `ToUi` messages from every
//! `tflick` and draws the suggestion popup next to the terminal cursor.
//!
//! Layers, outer to inner:
//! - `main`: starts the app and the popup window.
//! - `controller`: polling loop, the only place that combines IPC, Accessibility and the window.
//! - `ipc`, `macos`: I/O and OS glue (all `unsafe` lives in `macos`).
//! - `view`: what the popup looks like.
//! - `follow`, `layout`: pure decisions and geometry, unit tested.

/// Diagnostics on stderr: `TFLICK_DEBUG=1 cargo run -p tflick-ui`.
macro_rules! debug {
    ($($arg:tt)*) => {
        if std::env::var_os("TFLICK_DEBUG").is_some() {
            eprintln!("tflick-ui: {}", format_args!($($arg)*));
        }
    };
}

// After `debug!` so the modules can use it.
mod controller;
mod follow;
mod ipc;
mod layout;
mod macos;
mod view;

use gpui::{
    App, Application, Bounds, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
    point, prelude::*, px, size,
};

use layout::{POPUP_H, POPUP_W};
use view::{Assets, Popup};

/// Tuning knobs, overridable without rebuilding: `TFLICK_BLUR=60 TFLICK_TINT=230`.
fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let rx = ipc::listen();
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
            |window, cx| {
                let ns_window = macos::ns_window(window).expect("gpui window has an NSWindow");
                macos::set_blur(&ns_window, env_or("TFLICK_BLUR", 30));
                macos::hide(&ns_window);
                cx.new(|cx| {
                    cx.spawn_in(window, async move |view, cx| {
                        controller::run(ns_window, rx, view, cx).await
                    })
                    .detach();
                    Popup::new()
                })
            },
        )
        .unwrap();
    });
}
