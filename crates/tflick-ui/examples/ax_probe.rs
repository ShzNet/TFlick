//! Dumps what Accessibility exposes for an app, to debug caret lookup.
//! `cargo run -p tflick-ui --example ax_probe [-- <pid>]` (default: the frontmost app)

use accessibility_sys::*;
use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::string::CFString;

unsafe fn attr(el: AXUIElementRef, name: &str) -> Option<CFType> {
    let name = CFString::new(name);
    let mut v: CFTypeRef = std::ptr::null();
    let err = unsafe { AXUIElementCopyAttributeValue(el, name.as_concrete_TypeRef(), &mut v) };
    (err == kAXErrorSuccess && !v.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(v) })
}

unsafe fn describe(el: AXUIElementRef) -> String {
    let s = |n| {
        unsafe { attr(el, n) }
            .and_then(|v| v.downcast::<CFString>())
            .map(|s| s.to_string())
            .unwrap_or_default()
    };
    let classes = unsafe { attr(el, "AXDOMClassList") }
        .filter(|v| v.instance_of::<CFArray>())
        .map(|v| unsafe { CFArray::<CFType>::wrap_under_get_rule(v.as_CFTypeRef() as CFArrayRef) })
        .map(|a| {
            a.iter()
                .filter_map(|c| c.downcast::<CFString>())
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let focused = unsafe { attr(el, "AXFocused") }
        .and_then(|v| v.downcast::<CFBoolean>())
        .map(bool::from);
    format!(
        "role={} focused={focused:?} classes=[{classes}]",
        s("AXRole")
    )
}

unsafe fn walk(el: AXUIElementRef, depth: usize, budget: &mut usize) {
    if *budget == 0 || depth > 40 {
        return;
    }
    *budget -= 1;
    let d = unsafe { describe(el) };
    if d.contains("xterm") || d.contains("AXTextArea") || d.contains("AXTextField") || depth < 3 {
        println!("{:indent$}{d}", "", indent = depth * 2);
    }
    let Some(kids) = (unsafe { attr(el, "AXChildren") }) else {
        return;
    };
    let kids: CFArray<CFType> =
        unsafe { CFArray::wrap_under_get_rule(kids.as_CFTypeRef() as CFArrayRef) };
    for k in kids.iter() {
        unsafe { walk(k.as_CFTypeRef() as AXUIElementRef, depth + 1, budget) };
    }
}

fn main() {
    // Default: the frontmost app, i.e. the terminal this probe is run from.
    let pid: i32 = match std::env::args().nth(1) {
        Some(p) => p.parse().expect("pid"),
        None => objc2_app_kit::NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .expect("frontmost app")
            .processIdentifier(),
    };
    println!("pid {pid}");
    unsafe {
        let app = AXUIElementCreateApplication(pid);
        let name = CFString::new("AXManualAccessibility");
        let err = AXUIElementSetAttributeValue(
            app,
            name.as_concrete_TypeRef(),
            CFBoolean::true_value().as_CFTypeRef(),
        );
        println!("set AXManualAccessibility -> {err}");
        std::thread::sleep(std::time::Duration::from_millis(1500));
        match attr(app, "AXFocusedUIElement") {
            Some(f) => println!(
                "focused element: {}",
                describe(f.as_CFTypeRef() as AXUIElementRef)
            ),
            None => println!("focused element: none"),
        }
        if let Some(w) = attr(app, "AXFocusedWindow") {
            let mut budget = 20000;
            walk(w.as_CFTypeRef() as AXUIElementRef, 0, &mut budget);
            println!("visited {} nodes", 20000 - budget);
        }
    }
}
