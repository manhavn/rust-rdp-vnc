use std::sync::Arc;

/// Platform-neutral callbacks for RDP/VNC session events.
///
/// State codes match the Android app:
/// - 0 = IDLE / disconnected
/// - 1 = CONNECTING
/// - 2 = CONNECTED
/// - 3 = FAILED
pub trait SessionCallback: Send + Sync {
    fn on_state_changed(&self, state: i32, message: &str);
    fn on_frame_decoded(&self, pixels: &[i32], x: i32, y: i32, width: i32, height: i32);
    fn on_resolution_changed(&self, width: i32, height: i32);
    fn on_cursor_changed(&self, _cursor_type: i32) {}
    fn on_cursor_bitmap(
        &self,
        _width: i32,
        _height: i32,
        _hot_x: i32,
        _hot_y: i32,
        _pixels: &[i32],
    ) {
    }
}

pub type SharedCallback = Arc<dyn SessionCallback>;

pub fn is_rust_log_message(message: &str) -> bool {
    message.trim_start().to_ascii_lowercase().starts_with("[rust log]")
}

pub fn notify_state_change(callback: &dyn SessionCallback, state: i32, message: &str) {
    if is_rust_log_message(message) && crate::is_rust_log_disabled() {
        return;
    }
    callback.on_state_changed(state, message);
}

pub fn notify_resolution_change(callback: &dyn SessionCallback, width: i32, height: i32) {
    callback.on_resolution_changed(width, height);
}

pub fn push_frame(callback: &dyn SessionCallback, pixels: &[i32], width: i32, height: i32) {
    callback.on_frame_decoded(pixels, 0, 0, width, height);
}

pub fn notify_cursor_change(callback: &dyn SessionCallback, cursor_type: i32) {
    callback.on_cursor_changed(cursor_type);
}

pub fn notify_cursor_bitmap(
    callback: &dyn SessionCallback,
    width: i32,
    height: i32,
    hot_x: i32,
    hot_y: i32,
    pixels: &[i32],
) {
    callback.on_cursor_bitmap(width, height, hot_x, hot_y, pixels);
}
