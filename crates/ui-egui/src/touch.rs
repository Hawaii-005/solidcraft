//! Whether the UI is being driven by fingers rather than a mouse and keyboard.
//!
//! A sketch tool puts its value boxes on the point just picked and focuses them, so a number typed
//! straight after the pick sets the value. On a tablet that focus is wrong: the soft keyboard opens
//! over the drawing area after every pick, and the next tap lands on the keyboard instead of on the
//! canvas — the shape never gets its second point. With this on, the boxes stay visible and editable
//! but only take the keyboard when the box itself is tapped.
//!
//! On by default in Android builds; `SOLIDCRAFT_TOUCH_UI=1` turns it on elsewhere (the desktop shell
//! with a touch screen, or a test), `=0` turns it off on a tablet driven by a real keyboard.

use std::sync::OnceLock;

pub fn touch_ui() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| match std::env::var("SOLIDCRAFT_TOUCH_UI") {
        Ok(v) => v != "0",
        Err(_) => cfg!(target_os = "android"),
    })
}
