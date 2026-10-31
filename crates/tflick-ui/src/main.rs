//! `tflick-ui`: one daemon per user. Listens on a unix socket for `ToUi` messages from every
//! `tflick` and draws the suggestion popup next to the terminal cursor.
//!
//! Layers, outer to inner:
//! - `main`: starts the app and the popup window.
//! - `controller`: polling loop, the only place that combines IPC, Accessibility and the window.
//! - `ipc`, `macos`: I/O and OS glue (all `unsafe` lives in `macos`).
//! - `view`, `settings_view`: what the popup and the settings window look like.
//! - `settings`: the user's settings (a gpui global, saved as JSON); `login`: launch at login.
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
mod login;
mod macos;
mod settings;
mod settings_view;
mod view;

use gpui::{
    App, Application, Bounds, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
    point, prelude::*, px, size,
};

use settings::Settings;
use view::{Assets, Popup};

fn main() {
    let rx = ipc::listen();
    Application::new().with_assets(Assets).run(|cx: &mut App| {
        macos::hide_from_dock();
        let settings = Settings::load();
        let bounds = Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(settings.width as f32), px(settings.height as f32)),
        };
        cx.set_global(settings);
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
                macos::hide(&ns_window);
                cx.new(|cx| {
                    cx.spawn_in(window, async move |view, cx| {
                        controller::run(ns_window, rx, view, cx).await
                    })
                    .detach();
                    Popup::new(cx)
                })
            },
        )
        .unwrap();
        // First run, or the permission was removed: show where to grant it.
        if !macos::accessibility_trusted(true) {
            eprintln!(
                "tflick-ui: needs Accessibility permission to find the terminal window.\n\
                 Allow it in System Settings > Privacy & Security > Accessibility, then restart."
            );
            settings_view::open(cx, Some(settings_view::Page::Integrations));
        }
    });
}
