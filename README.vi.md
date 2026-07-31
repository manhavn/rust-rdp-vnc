# Rust RDP VNC

[English](README.md)

**Rust RDP VNC** là ứng dụng remote desktop dùng **core Rust** dùng chung, có hai bản:

| Client | Nền tảng | Giao diện |
|--------|----------|-----------|
| App iOS | iOS 15.0+ | Swift + SwiftUI (C FFI Bridge) |
| App Android | Android 8+ (API 26+) | Kotlin + Jetpack Compose |
| App desktop | Linux | Rust + egui / eframe |

Giao thức: **Microsoft RDP** (qua [IronRDP](https://github.com/Devolutions/IronRDP)) và **VNC** (qua [vnc-rs](https://crates.io/crates/vnc-rs)).

---

## Tính năng

- Phiên RDP và VNC trên cùng một codebase
- Backend Rust dùng chung: kết nối, giải mã hình, chuột, bàn phím
- File kết nối (`.rdp` / `.vnc`) mở/lưu trên các nền tảng
- iOS: Giao diện SwiftUI hiện đại, cử chỉ chạm, trackpad ảo, bàn phím ảo, phím tắt Ctrl+Alt+Del
- Android: cử chỉ chạm, bàn phím ảo, xem remote fullscreen
- Desktop Linux: menu, thanh công cụ, panel kết nối, zoom, fullscreen, hộp thoại file native

---

## Cấu trúc thư mục

```
rust-rdp/
├── rust_rdp/          # Thư viện Rust dùng chung (core RDP/VNC)
│   └── src/
│       ├── lib.rs     # API phiên làm việc
│       ├── callback.rs
│       ├── c_ffi.rs       # Cầu nối C ABI cho iOS / Swift
│       └── android_jni.rs # Cầu JNI (feature = "android")
├── ios/               # Ứng dụng iOS (SwiftUI + Xcode)
│   ├── RustRdpVnc/
│   │   ├── Bridge/    # File C Header & Wrapper RdpClient.swift
│   │   ├── Models/    # Model ConnectionProfile
│   │   ├── Views/     # Giao diện SwiftUI (Form, Canvas, Bookmarks, Settings)
│   │   └── Resources/ # Info.plist
│   └── RustRdpVnc.xcodeproj
├── android/           # Ứng dụng Android
├── desktop/           # Ứng dụng desktop Linux
│   ├── assets/        # Icon + file .desktop
│   └── src/
├── build-ios.sh       # Script build iOS (Rust static lib + Xcode)
├── build-dev.sh       # Build APK debug
├── build-release.sh   # Build APK release
├── build-desktop.sh   # Build binary Linux
└── generate_icon.py   # Sinh icon launcher Android
```

Thành viên Cargo workspace: `rust_rdp`, `desktop`.

---

## Yêu cầu

### Chung

- [Rust](https://rustup.rs/) (stable)
- Mạng để tải IronRDP (dependency git)

### Android

- Android SDK / NDK (đường dẫn NDK xem trong `android/app/build.gradle.kts`)
- [cargo-ndk](https://github.com/bbqsrc/cargo-ndk)
- Gradle
- Target: `aarch64-linux-android`, `x86_64-linux-android`

### Desktop (Linux)

- OpenGL / EGL (tùy backend eframe)
- Stack GUI thông thường: X11 hoặc Wayland, `libxkbcommon`, …
- Hộp thoại file (`rfd`) thường dùng portal / GTK

---

## Build — iOS

```bash
# Build thư viện Rust static cho iOS (thiết bị)
./build-ios.sh release aarch64-apple-ios

# Build cho iOS Simulator
./build-ios.sh release aarch64-apple-ios-sim
```

Để build file `.app` hoặc chạy ứng dụng iOS trên thiết bị bằng Xcode (trên macOS):
```bash
open ios/RustRdpVnc.xcodeproj
```

---

## Build — Android

```bash
# APK debug
./build-dev.sh

# APK release
./build-release.sh
```

Ví dụ đường dẫn output:

- Debug: `android/app/build/outputs/apk/debug/app-debug.apk`
- Release: `android/app/build/outputs/apk/release/app-release.apk` (nếu đã ký)

Task Gradle `buildRust` build thư viện Rust với `--features android` và copy file `.so` vào `jniLibs`.

Sửa `ANDROID_NDK_HOME` / đường dẫn NDK trong `android/app/build.gradle.kts` nếu SDK của bạn khác.

---

## Build — Desktop (Linux)

```bash
# Release
./build-desktop.sh

# Debug
./build-desktop.sh dev

# Hoặc gọi cargo trực tiếp
cargo build -p rust-rdp-vnc-desktop --release
cargo run -p rust-rdp-vnc-desktop
```

Binary:

- Release: `target/release/rust-rdp-vnc`
- Debug: `target/debug/rust-rdp-vnc`

---

## Sử dụng — Desktop

1. Nhập host, port, tài khoản (và domain nếu dùng RDP).
2. Chọn **RDP** hoặc **VNC**.
3. Bấm **Connect**, hoặc **Open** file `.rdp` / `.vnc` (tự kết nối).

### Phím tắt

| Phím tắt | Chức năng |
|----------|-----------|
| `Ctrl+O` | Mở file kết nối |
| `Ctrl+Return` | Kết nối |
| `Ctrl+S` | Lưu connection as… |
| `Ctrl+D` | Ngắt kết nối |
| `F11` | Toàn màn hình |
| `Ctrl++` / `Ctrl+-` | Phóng to / thu nhỏ |
| `Ctrl+scroll` | Zoom trên vùng màn hình remote |

### File kết nối

Định dạng tương thích client Android, ví dụ:

```text
full address:s:192.168.1.10:3389
username:s:user
password:s:secret
domain:s:WORKGROUP
connection mode:s:RDP
desktopwidth:i:1920
desktopheight:i:1080
```

- File RDP dùng đuôi `.rdp`.
- File VNC dùng đuôi `.vnc`.
- Nếu thiếu mode trong file, có thể suy ra từ đuôi file.

### Cài icon / mục menu (tuỳ chọn)

Tài nguyên nằm trong `desktop/assets/`.

```bash
# Binary vào PATH
cp target/release/rust-rdp-vnc ~/.local/bin/

# Icon
mkdir -p ~/.local/share/icons/hicolor/256x256/apps
cp desktop/assets/icon.png \
  ~/.local/share/icons/hicolor/256x256/apps/io.github.manhavn.rust-rdp-vnc.png

# Entry menu
mkdir -p ~/.local/share/applications
cp desktop/assets/io.github.manhavn.rust-rdp-vnc.desktop \
  ~/.local/share/applications/
# Nếu rust-rdp-vnc không nằm trong PATH, sửa Exec= thành đường dẫn tuyệt đối
```

---

## Sử dụng — Android

1. Build và cài APK.
2. Nhập thông tin kết nối hoặc mở file `.rdp` / `.vnc`.
3. Kết nối và dùng cử chỉ chạm cho chuột / zoom.

Các trường kết nối gần nhất có thể được lưu trong preferences của app.

---

## Kiến trúc (tóm tắt)

- **`rust_rdp`**: API phiên làm việc trung lập nền tảng (`connect_session`, input) và trait `SessionCallback` cho frame / trạng thái.
- **Android**: JNI (feature `android`) → UI Compose.
- **Desktop**: UI `eframe` implement `SessionCallback` và gọi cùng API phiên.
- RDP dùng TLS; verifier hiện tại chấp nhận chứng chỉ server không kiểm PKI (tiện cho lab, chưa harden cho mạng không tin cậy).

---

## Phát triển

```bash
# Kiểm tra core (không Android)
cargo check -p rust_rdp

# Core kèm binding JNI
cargo check -p rust_rdp --features android

# Chỉ desktop
cargo check -p rust-rdp-vnc-desktop
```

Sinh icon launcher Android:

```bash
python3 generate_icon.py
```

---

## Đóng gói (Snap / Flatpak)

Cấu hình nằm trong `snap/` và `flatpak/`. Script hỗ trợ:

```bash
# Snap Store — build + upload (mặc định: edge)
./scripts/publish-snap.sh
./scripts/publish-snap.sh --build-only
./scripts/publish-snap.sh beta

# Flatpak cài local
./scripts/publish-flatpak.sh
flatpak run io.github.manhavn.rust-rdp-vnc

# Flathub một lần (Podman): sinh sources + gói (+ tuỳ chọn PR GitHub)
./scripts/publish-flathub-podman.sh

# Hướng dẫn / chỉ generate sources
./scripts/publish-flatpak.sh --flathub-help
./scripts/publish-flatpak.sh --generate-sources
```

Tài liệu chi tiết:

- Snap Store: [snap/README.md](snap/README.md) · [snap/README.vi.md](snap/README.vi.md)
- Flathub: [flatpak/README.md](flatpak/README.md) · [flatpak/README.vi.md](flatpak/README.vi.md)

---

## Giấy phép / bên thứ ba

Dự án phụ thuộc các crate mã nguồn mở như IronRDP, vnc-rs, rustls, eframe. Khi phân phối lại, hãy tuân thủ giấy phép tương ứng của chúng.
