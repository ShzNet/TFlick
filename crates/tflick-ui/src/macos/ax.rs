//! Accessibility: which app has focus, its focused window, and the xterm.js caret.

use accessibility_sys::{
    AXIsProcessTrustedWithOptions, AXUIElementCopyAttributeValue, AXUIElementCreateApplication,
    AXUIElementCreateSystemWide, AXUIElementGetPid, AXUIElementRef, AXUIElementSetAttributeValue,
    AXValueGetValue, AXValueRef, kAXErrorSuccess, kAXFocusedApplicationAttribute,
    kAXFocusedUIElementAttribute, kAXFocusedWindowAttribute, kAXPositionAttribute,
    kAXSizeAttribute, kAXTrustedCheckOptionPrompt, kAXValueTypeCGPoint, kAXValueTypeCGSize,
};
use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFRelease, CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;

use crate::layout::Rect;

/// Whether we may read other apps' windows. With `prompt`, macOS shows its permission dialog.
pub fn accessibility_trusted(prompt: bool) -> bool {
    // SAFETY: kAXTrustedCheckOptionPrompt is a valid static CFString from the framework.
    let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::from(prompt))]);
    // SAFETY: plain call with a valid CFDictionary.
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

/// Where the user is typing, as precisely as we can tell.
#[derive(Debug)]
pub enum Anchor {
    /// The cursor cell itself (xterm.js terminals: VSCode, Cursor, Hyper…).
    Caret(Rect),
    /// Only the terminal window; the cursor cell has to be inferred from the grid.
    Window(Rect),
}

/// The terminal's focused window and where the user types in it.
pub struct Located {
    pub anchor: Anchor,
    /// The focused AX window element, to notice a switch to another window of the same app
    /// (compared with CFEqual).
    pub window: CFType,
    /// Its frame: a change means the window is being dragged or resized.
    pub frame: Rect,
}

/// Pid of the app that has keyboard focus, asked live from Accessibility.
/// (`NSWorkspace.frontmostApplication` is a cache updated by notifications and can lag.)
pub fn focused_app_pid() -> Option<i32> {
    // SAFETY: the system-wide element and the copied app element are released exactly once.
    unsafe {
        let system = AXUIElementCreateSystemWide();
        let app = copy_attr(system, kAXFocusedApplicationAttribute);
        CFRelease(system as CFTypeRef);
        let app = app?;
        let mut pid = 0;
        let err = AXUIElementGetPid(app as AXUIElementRef, &mut pid);
        CFRelease(app);
        (err == kAXErrorSuccess).then_some(pid)
    }
}

/// Locates the terminal the user is typing in, app `pid`. With `wake_electron`, (re)asks
/// Electron apps to expose their AX tree when no xterm.js caret is found; only needed while
/// typing, not on every follow-up poll.
pub fn locate_terminal(pid: i32, wake_electron: bool) -> Option<Located> {
    // SAFETY: every Copy/Create result is released exactly once, or owned by a CFType.
    unsafe {
        let app = AXUIElementCreateApplication(pid);
        let window =
            copy_attr(app, kAXFocusedWindowAttribute).map(|w| CFType::wrap_under_create_rule(w));
        let caret = copy_attr(app, kAXFocusedUIElementAttribute).and_then(|el| {
            let caret = xterm_caret(el as AXUIElementRef);
            if caret.is_none() && wake_electron {
                debug!(
                    "focused element is not an xterm.js caret: {}",
                    describe(el as _)
                );
            }
            CFRelease(el);
            caret
        });
        if caret.is_none() && wake_electron {
            // Electron apps (VSCode…) only expose their AX tree once asked, and build it
            // asynchronously, so ask again on every miss: the next keystroke will find it.
            // Harmless (an error code) for non-Electron apps.
            let attr = CFString::from_static_string("AXManualAccessibility");
            AXUIElementSetAttributeValue(
                app,
                attr.as_concrete_TypeRef(),
                CFBoolean::true_value().as_CFTypeRef(),
            );
        }
        CFRelease(app as CFTypeRef);
        let window = window?;
        let window_frame = frame(window.as_CFTypeRef() as AXUIElementRef)?;
        let anchor = match caret {
            Some(c) => Anchor::Caret(c),
            None => Anchor::Window(window_frame),
        };
        Some(Located {
            anchor,
            window,
            frame: window_frame,
        })
    }
}

/// Role and DOM classes of an element, for TFLICK_DEBUG logs.
unsafe fn describe(element: AXUIElementRef) -> String {
    // SAFETY: +1 results wrapped under the create rule, released on drop.
    let role = unsafe { copy_attr(element, "AXRole") }
        .map(|v| unsafe { CFType::wrap_under_create_rule(v) })
        .and_then(|v| v.downcast::<CFString>())
        .map(|s| s.to_string());
    format!("role={role:?} classes={:?}", unsafe {
        dom_classes(element)
    })
}

/// The element's HTML classes, for web content (Electron); empty for native views.
unsafe fn dom_classes(element: AXUIElementRef) -> Vec<String> {
    // SAFETY: a +1 value from copy_attr, released when `value` drops; only read as an array
    // after checking its type.
    let Some(value) = (unsafe { copy_attr(element, "AXDOMClassList") })
        .map(|v| unsafe { CFType::wrap_under_create_rule(v) })
    else {
        return Vec::new();
    };
    if !value.instance_of::<CFArray>() {
        return Vec::new();
    }
    let classes =
        unsafe { CFArray::<CFType>::wrap_under_get_rule(value.as_CFTypeRef() as CFArrayRef) };
    classes
        .iter()
        .filter_map(|c| c.downcast::<CFString>())
        .map(|c| c.to_string())
        .collect()
}

/// xterm.js keeps a hidden `xterm-helper-textarea` on the cursor cell (for IME), so its frame
/// is the caret. It is the app's focused element while the user types in the terminal.
/// port: macos-utils/src/window_server/ui_element.rs `find_x_term_caret_tree`.
unsafe fn xterm_caret(element: AXUIElementRef) -> Option<Rect> {
    // SAFETY: `element` is a valid AX element owned by the caller.
    unsafe {
        let is_xterm = dom_classes(element)
            .iter()
            .any(|c| c == "xterm-helper-textarea");
        if is_xterm { frame(element) } else { None }
    }
}

/// Position and size of an AX element, in global top-left coordinates.
unsafe fn frame(element: AXUIElementRef) -> Option<Rect> {
    // SAFETY: Copy results are released exactly once; AXValue reads write into correctly sized
    // #[repr(C)] structs.
    unsafe {
        let pos = copy_attr(element, kAXPositionAttribute);
        let size = copy_attr(element, kAXSizeAttribute);
        let r = match (pos, size) {
            (Some(pos), Some(size)) => {
                let mut p = CGPoint { x: 0.0, y: 0.0 };
                let mut s = CGSize { w: 0.0, h: 0.0 };
                let ok =
                    AXValueGetValue(pos as AXValueRef, kAXValueTypeCGPoint, (&raw mut p).cast())
                        && AXValueGetValue(
                            size as AXValueRef,
                            kAXValueTypeCGSize,
                            (&raw mut s).cast(),
                        );
                ok.then_some(Rect {
                    x: p.x,
                    y: p.y,
                    w: s.w,
                    h: s.h,
                })
            }
            _ => None,
        };
        for v in [pos, size].into_iter().flatten() {
            CFRelease(v);
        }
        r
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
