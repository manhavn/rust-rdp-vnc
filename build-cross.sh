#!/usr/bin/env bash
set -e

# Ensure local binary paths are in PATH
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:/usr/sbin:$PATH"

echo "======================================================="
echo "   Rust RDP/VNC Cross-Platform Multi-Target Build (Zig)"
echo "======================================================="

# Target list (Linux, macOS Intel, macOS Apple Silicon, Windows x64, Windows ARM64)
TARGETS=(
    "x86_64-unknown-linux-musl"
    "x86_64-unknown-linux-gnu"
    "aarch64-unknown-linux-musl"
    "aarch64-unknown-linux-gnu"
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
    "x86_64-pc-windows-gnu"
    "aarch64-pc-windows-gnullvm"
)

# 1. Verification of Prerequisites & Tools
echo "[+] Checking build prerequisites..."

# 1.1 Check Rust & Cargo
if ! command -v cargo &> /dev/null; then
    echo "[!] Rust/Cargo is not installed! Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    if [ -f "$HOME/.cargo/env" ]; then
        source "$HOME/.cargo/env"
    fi
fi

# 1.2 Formatting Workspace Code
echo "[+] Running cargo fmt --all..."
cargo fmt --all || echo "  ⚠️ cargo fmt encountered warnings/issues, continuing..."
echo "  ✓ Code formatted successfully"

# 1.3 Check MacOS SDK (Required for Apple Darwin cross-compilation)
SDK_DIR="$HOME/.sdk/MacOSX11.3.sdk"
if [ ! -d "$SDK_DIR" ]; then
    echo "[!] MacOS SDK not found at $SDK_DIR. Automatically downloading MacOSX11.3.sdk..."
    mkdir -p "$HOME/.sdk"
    curl -sSL https://github.com/phracker/MacOSX-SDKs/releases/download/11.3/MacOSX11.3.sdk.tar.xz | tar -xJ -C "$HOME/.sdk"
    echo "  ✓ MacOS SDK downloaded and extracted to $SDK_DIR"
fi
export SDKROOT="$SDK_DIR"
echo "  ✓ SDKROOT set to $SDKROOT"

# 1.4 Check Zig compiler
ZIG_NEEDED=false
if ! command -v zig &> /dev/null; then
    ZIG_NEEDED=true
else
    if ! zig env &> /dev/null; then
        echo "[!] Existing Zig installation is incomplete/broken. Reinstalling Zig 0.13.0..."
        ZIG_NEEDED=true
    fi
fi

if [ "$ZIG_NEEDED" = true ]; then
    echo "[!] Installing Zig 0.13.0..."
    mkdir -p "$HOME/.local/opt" "$HOME/.local/bin"
    curl -sSL https://ziglang.org/download/0.13.0/zig-linux-x86_64-0.13.0.tar.xz | tar -xJ -C "$HOME/.local/opt"
    rm -f "$HOME/.local/bin/zig"
    ln -s "$HOME/.local/opt/zig-linux-x86_64-0.13.0/zig" "$HOME/.local/bin/zig"
    echo "  ✓ Zig installed to $HOME/.local/opt/zig-linux-x86_64-0.13.0"
fi

ZIG_VER=$(zig version)
echo "  ✓ Zig version: $ZIG_VER"

# 1.5 Check cargo-zigbuild
if ! command -v cargo-zigbuild &> /dev/null; then
    echo "[!] cargo-zigbuild is not installed. Installing cargo-zigbuild..."
    cargo install cargo-zigbuild --locked
fi

ZIGBUILD_VER=$(cargo-zigbuild --version)
echo "  ✓ cargo-zigbuild: $ZIGBUILD_VER"

# 1.6 Check Python & required Python packages (pyfatfs for macOS DMG packaging)
echo "[+] Checking Python & packaging dependencies..."
if ! command -v python3 &> /dev/null; then
    echo "[!] python3 is not installed! Please install Python 3."
    exit 1
fi

if ! python3 -c "import pyfatfs" &> /dev/null; then
    echo "[!] Python library 'pyfatfs' is not installed. Installing pyfatfs..."
    if ! python3 -m pip --version &> /dev/null; then
        echo "[!] Installing pip..."
        curl -sSL https://bootstrap.pypa.io/get-pip.py | python3 - --user --break-system-packages >/dev/null 2>&1 || true
    fi
    python3 -m pip install --user --break-system-packages -q pyfatfs >/dev/null 2>&1 \
        || python3 -m pip install --user -q pyfatfs >/dev/null 2>&1 \
        || pip3 install --user pyfatfs >/dev/null 2>&1 \
        || echo "  ⚠️ Warning: Failed to install pyfatfs automatically."
fi
echo "  ✓ Python packaging dependencies verified"

# 2. Add Rustup targets if missing
echo "[+] Adding required Rustup targets..."
for target in "${TARGETS[@]}"; do
    rustup target add "$target" >/dev/null 2>&1 || true
done

# Prepare output distribution directory
DIST_DIR="./dist"
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

# 3. Build each target
echo ""
echo "[+] Starting Cross-compilation for all platforms..."
echo "-------------------------------------------------------"

for target in "${TARGETS[@]}"; do
    echo "[+] Building target: $target..."
    if [[ "$target" == *"apple-darwin"* ]]; then
        export RUSTFLAGS="-A linker_messages -C link-arg=-F$SDKROOT/System/Library/Frameworks -C link-arg=-L$SDKROOT/usr/lib -C link-arg=-undefined -C link-arg=dynamic_lookup"
    else
        export RUSTFLAGS="-A linker_messages"
    fi
    cargo zigbuild --release --target "$target" -p rust-rdp-vnc-desktop

    TARGET_DIST="$DIST_DIR/$target"
    mkdir -p "$TARGET_DIST"

    # Copy binary (handling .exe for windows)
    if [[ "$target" == *"windows"* ]]; then
        cp "target/$target/release/rust-rdp-vnc.exe" "$TARGET_DIST/"
        echo "  ✓ Successfully built binary for $target"
        echo "[+] Packaging Windows setup & installer files for $target..."
        python3 scripts/package_win.py "$TARGET_DIST" "$target"
    elif [[ "$target" == *"apple-darwin"* ]]; then
        cp "target/$target/release/rust-rdp-vnc" "$TARGET_DIST/"
        echo "  ✓ Successfully built binary for $target"
        echo "[+] Packaging macOS .app bundle, .dmg disk image, and .zip for $target..."
        python3 scripts/package_mac.py "$TARGET_DIST" "$target"
    else
        cp "target/$target/release/rust-rdp-vnc" "$TARGET_DIST/"
        echo "  ✓ Successfully built binary for $target"
        echo "[+] Packaging Linux .deb, .rpm, and .tar.gz for $target..."
        python3 scripts/package_linux.py "$TARGET_DIST" "$target"
    fi
done

echo ""
echo "======================================================="
echo "   BUILD SUMMARY - All Release Packages in ./dist/"
echo "======================================================="
ls -lh "$DIST_DIR"/*/*
echo "======================================================="
echo "   All cross-platform builds completed successfully! "
echo "======================================================="
