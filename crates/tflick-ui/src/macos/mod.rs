//! All the unsafe macOS glue, split by API.
//!
//! Coordinates: AX and gpui use a global space with the origin at the TOP-left of the primary
//! screen, y down. Cocoa (NSWindow, NSScreen) uses the BOTTOM-left, y up. Everything exported
//! here uses the top-left space.

mod ax;
mod window;

pub use ax::*;
pub use window::*;
