use jni::objects::{GlobalRef, JClass, JObject, JString};
use jni::sys::{jboolean, jint};
use jni::JNIEnv;
use std::sync::Arc;

use crate::{
    connect_session, disconnect_session, init_runtime, send_key_event, send_mouse_event,
    send_mouse_horizontal_wheel_event, send_mouse_wheel_event, send_scancode_event,
    SessionCallback,
};

struct JniCallback {
    jvm: Arc<jni::JavaVM>,
    callback: GlobalRef,
}

impl SessionCallback for JniCallback {
    fn on_state_changed(&self, state: i32, message: &str) {
        if let Ok(mut env) = self.jvm.attach_current_thread() {
            if let Ok(jmsg) = env.new_string(message) {
                let _ = env.call_method(
                    &self.callback,
                    "onStateChanged",
                    "(ILjava/lang/String;)V",
                    &[
                        jni::objects::JValue::Int(state),
                        jni::objects::JValue::Object(&jmsg),
                    ],
                );
            }
        }
    }

    fn on_frame_decoded(&self, pixels: &[i32], x: i32, y: i32, width: i32, height: i32) {
        if let Ok(mut attached_env) = self.jvm.attach_current_thread() {
            if let Ok(jarray) = attached_env.new_int_array(pixels.len() as i32) {
                if attached_env
                    .set_int_array_region(&jarray, 0, pixels)
                    .is_ok()
                {
                    let res = attached_env.call_method(
                        &self.callback,
                        "onFrameDecoded",
                        "([IIIII)V",
                        &[
                            jni::objects::JValue::Object(&jarray),
                            jni::objects::JValue::Int(x),
                            jni::objects::JValue::Int(y),
                            jni::objects::JValue::Int(width),
                            jni::objects::JValue::Int(height),
                        ],
                    );
                    if let Err(e) = res {
                        self.on_state_changed(
                            2,
                            &format!("[Rust Log] JNI onFrameDecoded error: {:?}", e),
                        );
                    }
                } else {
                    self.on_state_changed(2, "[Rust Log] JNI set_int_array_region failed");
                }
            } else {
                self.on_state_changed(2, "[Rust Log] JNI new_int_array failed");
            }
        }
    }

    fn on_resolution_changed(&self, width: i32, height: i32) {
        if let Ok(mut env) = self.jvm.attach_current_thread() {
            let _ = env.call_method(
                &self.callback,
                "onResolutionChanged",
                "(II)V",
                &[
                    jni::objects::JValue::Int(width),
                    jni::objects::JValue::Int(height),
                ],
            );
        }
    }

    fn on_cursor_changed(&self, cursor_type: i32) {
        if let Ok(mut env) = self.jvm.attach_current_thread() {
            let _ = env.call_method(
                &self.callback,
                "onCursorChanged",
                "(I)V",
                &[jni::objects::JValue::Int(cursor_type)],
            );
        }
    }

    fn on_cursor_bitmap(&self, width: i32, height: i32, hot_x: i32, hot_y: i32, pixels: &[i32]) {
        if let Ok(mut env) = self.jvm.attach_current_thread() {
            if let Ok(jarray) = env.new_int_array(pixels.len() as i32) {
                if env.set_int_array_region(&jarray, 0, pixels).is_ok() {
                    let _ = env.call_method(
                        &self.callback,
                        "onCursorBitmap",
                        "(IIII[I)V",
                        &[
                            jni::objects::JValue::Int(width),
                            jni::objects::JValue::Int(height),
                            jni::objects::JValue::Int(hot_x),
                            jni::objects::JValue::Int(hot_y),
                            jni::objects::JValue::Object(&jarray),
                        ],
                    );
                }
            }
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_rustai_rdp_RdpClient_initJni(_env: JNIEnv, _class: JClass) {
    init_runtime();
}

#[no_mangle]
pub extern "system" fn Java_com_rustai_rdp_RdpClient_connect(
    mut env: JNIEnv,
    _class: JClass,
    host: JString,
    port: jint,
    username: JString,
    password: JString,
    domain: JString,
    width: jint,
    height: jint,
    connection_mode: JString,
    callback: JObject,
) {
    let host_str: String = env.get_string(&host).unwrap().into();
    let user_str: String = env.get_string(&username).unwrap().into();
    let pass_str: String = env.get_string(&password).unwrap().into();
    let domain_str: String = env.get_string(&domain).unwrap().into();
    let conn_mode_str: String = env.get_string(&connection_mode).unwrap().into();
    let jvm = env.get_java_vm().unwrap();
    let callback_ref = env.new_global_ref(callback).unwrap();

    let cb: Arc<dyn SessionCallback> = Arc::new(JniCallback {
        jvm: Arc::new(jvm),
        callback: callback_ref,
    });

    connect_session(
        host_str,
        port,
        user_str,
        pass_str,
        domain_str,
        width,
        height,
        conn_mode_str,
        cb,
    );
}

#[no_mangle]
pub extern "system" fn Java_com_rustai_rdp_RdpClient_disconnect(_env: JNIEnv, _class: JClass) {
    disconnect_session();
}

#[no_mangle]
pub extern "system" fn Java_com_rustai_rdp_RdpClient_sendMouseEvent(
    _env: JNIEnv,
    _class: JClass,
    x: jint,
    y: jint,
    action: jint,
) {
    send_mouse_event(x, y, action);
}

#[no_mangle]
pub extern "system" fn Java_com_rustai_rdp_RdpClient_sendMouseWheelEvent(
    _env: JNIEnv,
    _class: JClass,
    x: jint,
    y: jint,
    units: jint,
) {
    send_mouse_wheel_event(x, y, units);
}

#[no_mangle]
pub extern "system" fn Java_com_rustai_rdp_RdpClient_sendMouseHorizontalWheelEvent(
    _env: JNIEnv,
    _class: JClass,
    x: jint,
    y: jint,
    units: jint,
) {
    send_mouse_horizontal_wheel_event(x, y, units);
}

#[no_mangle]
pub extern "system" fn Java_com_rustai_rdp_RdpClient_sendKeyEvent(
    _env: JNIEnv,
    _class: JClass,
    keycode: jint,
    pressed: jint,
) {
    send_key_event(keycode, pressed);
}

#[no_mangle]
pub extern "system" fn Java_com_rustai_rdp_RdpClient_sendScancodeEvent(
    _env: JNIEnv,
    _class: JClass,
    scancode: jint,
    is_extended: jboolean,
    pressed: jint,
) {
    send_scancode_event(scancode, is_extended != 0, pressed);
}
