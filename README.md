# Rust RDP VNC

[Tiếng Việt](README.vi.md)

**Rust RDP VNC** is a remote desktop client with a shared **Rust** protocol core, available as:

| Client | Platform | UI |
|--------|----------|-----|
| iOS app | iOS 15.0+ | Swift + SwiftUI (C FFI Bridge) |
| Android app | Android 8+ (API 26+) | Kotlin + Jetpack Compose |
| Desktop app | Linux | Rust + egui / eframe |

Protocols: **Microsoft RDP** (via [IronRDP](https://github.com/Devolutions/IronRDP)) and **VNC** (via [vnc-rs](https://crates.io/crates/vnc-rs)).

---

## Features

- RDP and VNC sessions from one codebase
- Shared Rust backend for connection, decoding, mouse, and keyboard
- Connection files (`.rdp` / `.vnc`) open/save across clients
- iOS: SwiftUI responsive interface, touch gestures, trackpad mode, soft keyboard, Ctrl+Alt+Del shortcut
- Android: touch gestures, soft keyboard helpers, fullscreen remote view
- Desktop Linux: menu bar, toolbar, connection panel, zoom, fullscreen, native file dialogs

---

## Repository layout

```
rust-rdp/
├── rust_rdp/          # Shared Rust library (RDP/VNC core)
│   └── src/
│       ├── lib.rs     # Session API
      ├── callback.rs
│       ├── c_ffi.rs       # C ABI bridge for iOS / Swift
│       └── android_jni.rs # JNI bridge (feature = "android")
├── ios/               # iOS Application (SwiftUI + Xcode)
│   ├── RustRdpVnc/
│   │   ├── Bridge/    # C FFI Bridging Headers & RdpClient.swift wrapper
│   │   ├── Models/    # ConnectionProfile model
│   │   ├── Views/     # SwiftUI Views (Form, Canvas, Bookmarks, Settings)
│   │   └── Resources/ # Info.plist
│   └── RustRdpVnc.xcodeproj
├── android/           # Android application
├── desktop/           # Linux desktop application
│   ├── assets/        # Icons + .desktop entry
│   └── src/
├── build-ios.sh       # iOS build script (Rust static lib + Xcode)
├── build-dev.sh       # Android debug APK
├── build-release.sh   # Android release APK
├── build-desktop.sh   # Linux desktop binary
└── generate_icon.py   # Android launcher icons
```

Cargo workspace members: `rust_rdp`, `desktop`.

---

## Requirements

### Common

- [Rust](https://rustup.rs/) (stable)
- Network access to fetch IronRDP (git dependency)

### Android

- Android SDK / NDK (see `android/app/build.gradle.kts` for NDK path)
- [cargo-ndk](https://github.com/bbqsrc/cargo-ndk)
- Gradle
- Targets: `aarch64-linux-android`, `x86_64-linux-android`

### Desktop (Linux)

- OpenGL / EGL (or Vulkan depending on eframe backend)
- Typical GUI stack: X11 or Wayland, `libxkbcommon`, etc.
- Optional: `rfd` file dialogs use the portal / GTK stack on most distros

---

## Build — iOS

```bash
# Build Rust static library for iOS
./build-ios.sh release aarch64-apple-ios

# Build for iOS Simulator
./build-ios.sh release aarch64-apple-ios-sim
```

To build the iOS `.app` or deploy to a device on macOS, open the Xcode project:
```bash
open ios/RustRdpVnc.xcodeproj
```

---

## Build — Android

```bash
# Debug APK
./build-dev.sh

# Release APK
./build-release.sh
```

Output examples:

- Debug: `android/app/build/outputs/apk/debug/app-debug.apk`
- Release: `android/app/build/outputs/apk/release/app-release.apk` (if signed)

The Gradle `buildRust` task builds the Rust library with `--features android` and copies `.so` files into `jniLibs`.

Adjust `ANDROID_NDK_HOME` / NDK path in `android/app/build.gradle.kts` if your SDK layout differs.

---

## Build — Desktop (Linux)

```bash
# Release
./build-desktop.sh

# Debug
./build-desktop.sh dev

# Or directly
cargo build -p rust-rdp-vnc-desktop --release
cargo run -p rust-rdp-vnc-desktop
```

Binary:

- Release: `target/release/rust-rdp-vnc`
- Debug: `target/debug/rust-rdp-vnc`

---

## Usage — Desktop

1. Enter host, port, credentials (and domain for RDP if needed).
2. Choose **RDP** or **VNC**.
3. Click **Connect**, or **Open** a `.rdp` / `.vnc` file (auto-connects).

### Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+O` | Open connection file |
| `Ctrl+Return` | Connect |
| `Ctrl+S` | Save connection as… |
| `Ctrl+D` | Disconnect |
| `F11` | Fullscreen |
| `Ctrl++` / `Ctrl+-` | Zoom in / out |
| `Ctrl+scroll` | Zoom over the remote view |

### Connection files

Format is compatible with the Android client, for example:

```text
full address:s:192.168.1.10:3389
username:s:user
password:s:secret
domain:s:WORKGROUP
connection mode:s:RDP
desktopwidth:i:1920
desktopheight:i:1080
```

- RDP files use the `.rdp` extension.
- VNC files use the `.vnc` extension.
- Mode can also be inferred from the file extension when missing.

### Install icon / menu entry (optional)

Assets live under `desktop/assets/`.

```bash
# Binary on PATH
cp target/release/rust-rdp-vnc ~/.local/bin/

# Icon
mkdir -p ~/.local/share/icons/hicolor/256x256/apps
cp desktop/assets/icon.png \
  ~/.local/share/icons/hicolor/256x256/apps/io.github.manhavn.rust-rdp-vnc.png

# Desktop entry
mkdir -p ~/.local/share/applications
cp desktop/assets/io.github.manhavn.rust-rdp-vnc.desktop \
  ~/.local/share/applications/
# Set Exec= to an absolute path if rust-rdp-vnc is not on PATH
```

---

## Usage — Android

1. Build and install the APK.
2. Enter connection details or open a `.rdp` / `.vnc` file.
3. Connect and use touch gestures for mouse / zoom.

Last connection fields can be stored in app preferences.

---

## Architecture notes

- **`rust_rdp`**: platform-neutral session API (`connect_session`, input helpers) plus `SessionCallback` for frames and state.
- **Android**: JNI (`android` feature) → Compose UI.
- **Desktop**: `eframe` UI implements `SessionCallback` and drives the same session API.
- RDP path uses TLS; the current verifier accepts server certificates without PKI validation (convenient for labs, not hardened for untrusted networks).

---

## Development

```bash
# Check core (no Android)
cargo check -p rust_rdp

# Check core with JNI bindings
cargo check -p rust_rdp --features android

# Desktop only
cargo check -p rust-rdp-vnc-desktop
```

Generate Android launcher icons:

```bash
python3 generate_icon.py
```

---

## Packaging (Snap / Flatpak)

Scaffolding lives under `snap/` and `flatpak/`. Helper scripts:

```bash
# Snap Store — build + upload (default channel: edge)
./scripts/publish-snap.sh
./scripts/publish-snap.sh --build-only
./scripts/publish-snap.sh beta

# Flatpak local install
./scripts/publish-flatpak.sh
flatpak run io.github.manhavn.rust-rdp-vnc

# Flathub one-shot (Podman): generate sources + package (+ optional GitHub PR)
./scripts/publish-flathub-podman.sh

# Flathub help / generate sources only
./scripts/publish-flatpak.sh --flathub-help
./scripts/publish-flatpak.sh --generate-sources
```

Detailed guides:

- Snap Store: [snap/README.md](snap/README.md) · [snap/README.vi.md](snap/README.vi.md)
- Flathub: [flatpak/README.md](flatpak/README.md) · [flatpak/README.vi.md](flatpak/README.vi.md)

---

## License / third party

This project depends on open-source crates including IronRDP, vnc-rs, rustls, and eframe. Respect their respective licenses when redistributing.
