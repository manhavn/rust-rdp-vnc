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
}

pub type SharedCallback = Arc<dyn SessionCallback>;

pub fn notify_state_change(callback: &dyn SessionCallback, state: i32, message: &str) {
    callback.on_state_changed(state, message);
}

pub fn notify_resolution_change(callback: &dyn SessionCallback, width: i32, height: i32) {
    callback.on_resolution_changed(width, height);
}

pub fn push_frame(callback: &dyn SessionCallback, pixels: &[i32], width: i32, height: i32) {
    callback.on_frame_decoded(pixels, 0, 0, width, height);
}
