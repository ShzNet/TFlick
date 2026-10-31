//! All the unsafe macOS glue: the NSWindow behind gpui, background blur, and Accessibility
//! lookups of the focused terminal window.
//!
//! Coordinates: AX and gpui use a global space with the origin at the TOP-left of the primary
//! screen, y down. Cocoa (NSWindow, NSScreen) uses the BOTTOM-left, y up.

use std::ffi::c_void;

use accessibility_sys::{
    AXIsProcessTrustedWithOptions, AXUIElementCopyAttributeValue, AXUIElementCreateApplication,
    AXUIElementRef, AXValueGetValue, AXValueRef, kAXErrorSuccess, kAXFocusedWindowAttribute,
    kAXPositionAttribute, kAXSizeAttribute, kAXTrustedCheckOptionPrompt, kAXValueTypeCGPoint,
    kAXValueTypeCGSize,
};
use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use gpui::Window;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSScreen, NSView, NSWindow, NSWorkspace,
};
use objc2_foundation::NSPoint;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// A rectangle in global top-left coordinates (AX / gpui space), in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

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

/// Whether we may read other apps' windows. With `prompt`, macOS shows its permission dialog.
pub fn accessibility_trusted(prompt: bool) -> bool {
    // SAFETY: kAXTrustedCheckOptionPrompt is a valid static CFString from the framework.
    let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::from(prompt))]);
    // SAFETY: plain call with a valid CFDictionary.
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
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

/// Frame of the focused window of the frontmost app (the terminal the user is typing in).
pub fn focused_window_frame() -> Option<Rect> {
    let pid = NSWorkspace::sharedWorkspace()
        .frontmostApplication()?
        .processIdentifier();
    // SAFETY: every Copy/Create result is released exactly once below; AXValue reads write into
    // correctly sized #[repr(C)] structs.
    unsafe {
        let app = AXUIElementCreateApplication(pid);
        let window = copy_attr(app, kAXFocusedWindowAttribute);
        CFRelease(app as CFTypeRef);
        let window = window? as AXUIElementRef;
        let pos = copy_attr(window, kAXPositionAttribute);
        let size = copy_attr(window, kAXSizeAttribute);
        CFRelease(window as CFTypeRef);
        let (pos, size) = (pos?, size?);
        let mut p = CGPoint { x: 0.0, y: 0.0 };
        let mut s = CGSize { w: 0.0, h: 0.0 };
        let ok = AXValueGetValue(pos as AXValueRef, kAXValueTypeCGPoint, (&raw mut p).cast())
            && AXValueGetValue(size as AXValueRef, kAXValueTypeCGSize, (&raw mut s).cast());
        CFRelease(pos);
        CFRelease(size);
        ok.then_some(Rect {
            x: p.x,
            y: p.y,
            w: s.w,
            h: s.h,
        })
    }
}

#[repr(C)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
struct CGSize {
    w: f64,
    h: f64,
}

/// Caller owns (must CFRelease) the returned value.
unsafe fn copy_attr(element: AXUIElementRef, attr: &'static str) -> Option<CFTypeRef> {
    let attr = CFString::from_static_string(attr);
    let mut value: CFTypeRef = std::ptr::null();
    // SAFETY: valid element and attribute; on success `value` holds a +1 reference.
    let err =
        unsafe { AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef(), &mut value) };
    (err == kAXErrorSuccess && !value.is_null()).then_some(value)
}
