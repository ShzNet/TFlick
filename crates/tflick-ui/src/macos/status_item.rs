//! The menu bar icon: Settings… and Quit. The menu's actions post to the same channel as the
//! socket, so the controller handles "Settings…" exactly like `tflick settings`.

use std::sync::mpsc::Sender;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{
    AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel,
};
use objc2_app_kit::{NSApplication, NSImage, NSMenu, NSMenuItem, NSStatusBar};
use objc2_foundation::{NSData, NSObject, NSObjectProtocol, NSSize, NSString, ns_string};
use tflick_ipc::ToUi;

/// Length of a status item sized to its content (`NSVariableStatusItemLength`).
const VARIABLE_LENGTH: f64 = -1.0;

/// The SHZ mark (icons/shz-mark.svg) as a one-color template, 15pt at 2x; its center dot is a
/// hole since template images only use alpha.
const ICON: &[u8] = include_bytes!("../../icons/shz-menubar.png");

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and MenuTarget does not implement Drop.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Sender<ToUi>]
    struct MenuTarget;

    // SAFETY: NSObjectProtocol has no safety requirements.
    unsafe impl NSObjectProtocol for MenuTarget {}

    impl MenuTarget {
        // SAFETY: the signature matches an NSMenuItem action (one `sender` argument).
        #[unsafe(method(openSettings:))]
        fn open_settings(&self, _sender: Option<&AnyObject>) {
            let _ = self.ivars().send(ToUi::OpenSettings);
        }

        // SAFETY: as above.
        #[unsafe(method(quit:))]
        fn quit(&self, _sender: Option<&AnyObject>) {
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }
    }
);

/// Adds the menu bar icon. It stays for the life of the app.
pub fn install_status_item(tx: Sender<ToUi>) {
    let mtm = MainThreadMarker::new().expect("called on the main thread");
    let target = MenuTarget::alloc(mtm).set_ivars(tx);
    // SAFETY: plain NSObject init of a freshly allocated object.
    let target: Retained<MenuTarget> = unsafe { msg_send![super(target), init] };

    let menu = NSMenu::new(mtm);
    let item = |title: &NSString, action: Sel, key: &NSString| {
        // SAFETY: `action` names a method MenuTarget implements with the action signature, and
        // the target outlives the item (both are leaked below).
        unsafe {
            let item = NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                title,
                Some(action),
                key,
            );
            item.setTarget(Some(&target));
            item
        }
    };
    menu.addItem(&item(
        ns_string!("Cài đặt TFlick…"),
        sel!(openSettings:),
        ns_string!(","),
    ));
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&item(
        ns_string!("Thoát TFlick"),
        sel!(quit:),
        ns_string!("q"),
    ));

    let status = NSStatusBar::systemStatusBar().statusItemWithLength(VARIABLE_LENGTH);
    if let Some(button) = status.button(mtm) {
        // A template image follows the menu bar's light/dark look like system icons do.
        match NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(ICON)) {
            Some(image) => {
                image.setSize(NSSize::new(15.0, 15.0));
                image.setTemplate(true);
                button.setImage(Some(&image));
            }
            None => button.setTitle(ns_string!("TFlick")),
        }
    }
    status.setMenu(Some(&menu));
    // The status bar holds the item weakly and menu items hold their target weakly: keep both.
    std::mem::forget(status);
    std::mem::forget(target);
}
