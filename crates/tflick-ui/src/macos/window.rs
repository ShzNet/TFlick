//! Our own window and the screen: the NSWindow behind gpui, blur, moving, screens, mouse.

use std::ffi::c_void;

use gpui::Window;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSEvent, NSScreen, NSView, NSWindow,
};
use objc2_foundation::NSPoint;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::layout::Rect;

// SkyLight private API, same declaration gpui uses (gpui-0.2.2/src/platform/mac/window.rs:94).
// Also what Terminal.app, kitty and Alacritty use for background blur.
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGSMainConnectionID() -> *mut c_void;
    fn CGSSetWindowBackgroundBlurRadius(
        connection_id: *mut c_void,
        window_id: isize,
        radius: i64,
    ) -> i32;
}

/// Run as an accessory app: no Dock icon, no menu bar, never steals activation.
pub fn hide_from_dock() {
    let mtm = MainThreadMarker::new().expect("called on the main thread");
    NSApplication::sharedApplication(mtm)
        .setActivationPolicy(NSApplicationActivationPolicy::Accessory);
}

/// Blurs whatever is behind the window. gpui's `Blurred` uses a light NSVisualEffectView with no
/// radius control, so we keep the window `Transparent` and set the blur radius ourselves.
pub fn set_blur(ns_window: &NSWindow, radius: i64) {
    // SAFETY: plain C call on our own window number; returns an error code we can ignore.
    unsafe {
        CGSSetWindowBackgroundBlurRadius(CGSMainConnectionID(), ns_window.windowNumber(), radius);
    }
}

/// The NSWindow behind a gpui window.
/// ponytail: gpui 0.2.2 has no public move API, so we reach the NSWindow through the raw
/// handle. Drop this once gpui exposes `set_position`.
pub fn ns_window(window: &Window) -> Option<Retained<NSWindow>> {
    // gpui::Window has an inherent `window_handle()` too, so call the trait explicitly.
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(h) = handle.as_raw() else {
        return None;
    };
    // SAFETY: gpui hands out a valid NSView pointer for the lifetime of the window.
    let view: &NSView = unsafe { h.ns_view.cast().as_ref() };
    view.window()
}

/// Moves the window so its top-left corner is at `(x, y)` in global top-left coordinates, and
/// brings it to front without activating our app.
///
/// Must run OUTSIDE any gpui `update`: moving to a screen with another scale makes AppKit call
/// gpui's resize callback synchronously, and that callback fails while the window is borrowed
/// by an update, leaving gpui's scale factor stale (content drawn at half or double size).
pub fn show_at(ns_window: &NSWindow, x: f64, y: f64) {
    ns_window.setFrameTopLeftPoint(NSPoint::new(x, primary_screen_height() - y));
    if !ns_window.isVisible() {
        ns_window.orderFrontRegardless();
    }
}

pub fn hide(ns_window: &NSWindow) {
    ns_window.orderOut(None);
}

/// Whether the left mouse button is held right now (anywhere on screen), e.g. during a window
/// drag.
pub fn left_mouse_down() -> bool {
    NSEvent::pressedMouseButtons() & 1 != 0
}

/// Mouse pointer position, in global top-left coordinates. Live, unlike AX window frames.
pub fn mouse_location() -> (f64, f64) {
    let p = NSEvent::mouseLocation();
    (p.x, primary_screen_height() - p.y)
}

/// Visible frames (minus menu bar and Dock) of all screens, in global top-left coordinates.
pub fn screens() -> Vec<Rect> {
    let mtm = MainThreadMarker::new().expect("called on the main thread");
    let primary_h = primary_screen_height();
    NSScreen::screens(mtm)
        .iter()
        .map(|s| {
            let f = s.visibleFrame();
            Rect {
                x: f.origin.x,
                y: primary_h - (f.origin.y + f.size.height),
                w: f.size.width,
                h: f.size.height,
            }
        })
        .collect()
}

fn primary_screen_height() -> f64 {
    let mtm = MainThreadMarker::new().expect("called on the main thread");
    NSScreen::screens(mtm)
        .firstObject()
        .map_or(0.0, |s| s.frame().size.height)
}
