#ifndef RustRdpBridge_h
#define RustRdpBridge_h

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

// Callback definitions for C / Swift integration
typedef void (*StateChangedCallback)(void *user_data, int32_t state, const char *message);
typedef void (*FrameDecodedCallback)(void *user_data, const int32_t *pixels, int32_t len, int32_t x, int32_t y, int32_t width, int32_t height);
typedef void (*ResolutionChangedCallback)(void *user_data, int32_t width, int32_t height);

// Rust RDP / VNC Core C API
void rust_rdp_init(void);

uint64_t rust_rdp_connect(
    const char *host,
    int32_t port,
    const char *username,
    const char *password,
    const char *domain,
    int32_t width,
    int32_t height,
    const char *conn_mode,
    void *user_data,
    StateChangedCallback on_state_changed,
    FrameDecodedCallback on_frame_decoded,
    ResolutionChangedCallback on_resolution_changed
);

void rust_rdp_disconnect(void);

void rust_rdp_send_mouse_event(int32_t x, int32_t y, int32_t action);
void rust_rdp_send_mouse_wheel_event(int32_t x, int32_t y, int32_t units);
void rust_rdp_send_mouse_horizontal_wheel_event(int32_t x, int32_t y, int32_t units);
void rust_rdp_send_key_event(int32_t keycode, int32_t pressed);
void rust_rdp_send_scancode_event(int32_t scancode, int32_t is_extended, int32_t pressed);

#ifdef __cplusplus
}
#endif

#endif /* RustRdpBridge_h */
