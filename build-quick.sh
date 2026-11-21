#!/usr/bin/env bash
# Exit on error
set -e

GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'
BOLD='\033[1m'

echo -e "${CYAN}${BOLD}==================================================${NC}"
echo -e "${CYAN}${BOLD}   RUST RDP/VNC - QUICK BUILD & LINUX PACKAGING   ${NC}"
echo -e "${CYAN}${BOLD}==================================================${NC}"

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$DIR"

# ---------------------------------------------------------------------------
# Dependency bootstrap: install everything needed for the build and for
# producing both .deb and .rpm packages, without manual steps.
# ---------------------------------------------------------------------------
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/sbin:$PATH"

PM=""
for candidate in apt-get dnf pacman zypper apk; do
    if command -v "$candidate" >/dev/null 2>&1; then
        PM="$candidate"
        break
    fi
done

PRIV=""
if [ "$(id -u)" -ne 0 ] && command -v sudo >/dev/null 2>&1; then
    PRIV="sudo"
fi

APT_UPDATED=false

install_system_package() {
    local key="$1"
    local pkgs=""

    case "$PM|$key" in
        apt-get|python3) pkgs="python3" ;;
        apt-get|dpkg)    pkgs="dpkg" ;;
        apt-get|cc)      pkgs="build-essential" ;;
        apt-get|curl)    pkgs="curl" ;;
        dnf|python3)     pkgs="python3" ;;
        dnf|dpkg)        pkgs="dpkg" ;;
        dnf|cc)          pkgs="gcc gcc-c++ make" ;;
        dnf|curl)        pkgs="curl" ;;
        pacman|python3)  pkgs="python" ;;
        pacman|dpkg)     pkgs="dpkg" ;;
        pacman|cc)       pkgs="base-devel" ;;
        pacman|curl)     pkgs="curl" ;;
        zypper|python3)  pkgs="python3" ;;
        zypper|dpkg)     pkgs="dpkg" ;;
        zypper|cc)       pkgs="gcc gcc-c++ make" ;;
        zypper|curl)     pkgs="curl" ;;
        apk|python3)     pkgs="python3" ;;
        apk|dpkg)        pkgs="dpkg" ;;
        apk|cc)          pkgs="build-base" ;;
        apk|curl)        pkgs="curl" ;;
    esac

    if [ -z "$pkgs" ]; then
        echo "  ✘ No install rule for '$key' on package manager '${PM:-unknown}'."
        return 1
    fi
    if [ "$(id -u)" -ne 0 ] && [ -z "$PRIV" ]; then
        echo "  ✘ Root privileges required to install '$pkgs' (sudo not found)."
        return 1
    fi

    echo "  → Installing $pkgs via $PM..."
    case "$PM" in
        apt-get)
            if [ "$APT_UPDATED" != true ]; then
                $PRIV apt-get update || return 1
                APT_UPDATED=true
            fi
            $PRIV apt-get install -y $pkgs
            ;;
        dnf)    $PRIV dnf install -y $pkgs ;;
        pacman) $PRIV pacman -Sy --noconfirm --needed $pkgs ;;
        zypper) $PRIV zypper --non-interactive install $pkgs ;;
        apk)    $PRIV apk add $pkgs ;;
    esac
}

ensure_tool() {
    local cmd="$1"
    local key="$2"

    command -v "$cmd" >/dev/null 2>&1 && return 0

    echo "[!] Missing '$cmd' - installing automatically..."
    install_system_package "$key" || {
        echo "✘ Could not install '$cmd'. Install it manually and re-run."
        exit 1
    }
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "✘ '$cmd' is still missing after installation."
        exit 1
    fi
}

ensure_zig() {
    if command -v zig >/dev/null 2>&1 && zig version >/dev/null 2>&1; then
        return 0
    fi

    local zig_arch
    case "$(uname -m)" in
        x86_64|amd64)  zig_arch="x86_64" ;;
        aarch64|arm64) zig_arch="aarch64" ;;
        *)
            echo "✘ Unsupported host architecture for Zig: $(uname -m)"
            return 1
            ;;
    esac

    local zig_version="0.13.0"
    local zig_dir="$HOME/.local/opt/zig-linux-${zig_arch}-${zig_version}"

    echo "[!] Missing 'zig' - downloading Zig ${zig_version}..."
    mkdir -p "$HOME/.local/opt" "$HOME/.local/bin"
    curl -sSL "https://ziglang.org/download/${zig_version}/zig-linux-${zig_arch}-${zig_version}.tar.xz" \
        | tar -xJ -C "$HOME/.local/opt"
    ln -sf "$zig_dir/zig" "$HOME/.local/bin/zig"
    hash -r

    if ! command -v zig >/dev/null 2>&1; then
        echo "✘ Zig installation failed."
        return 1
    fi
}

ensure_cargo_zigbuild() {
    if command -v cargo-zigbuild >/dev/null 2>&1; then
        return 0
    fi

    echo "[!] Missing 'cargo-zigbuild' - installing via cargo..."
    if ! cargo install cargo-zigbuild --locked; then
        echo "✘ Could not install cargo-zigbuild."
        return 1
    fi
    hash -r

    if ! command -v cargo-zigbuild >/dev/null 2>&1; then
        echo "✘ 'cargo-zigbuild' is still missing after installation."
        return 1
    fi
}

ensure_tool curl curl
ensure_tool python3 python3
ensure_tool dpkg-deb dpkg
ensure_tool cc cc

# Rust toolchain (needed before any cargo/rustc invocations below)
if ! command -v cargo >/dev/null 2>&1 || ! command -v rustc >/dev/null 2>&1; then
    echo "[!] Rust toolchain not found - installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1090
    [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
fi
if ! command -v cargo >/dev/null 2>&1 || ! command -v rustc >/dev/null 2>&1; then
    echo "✘ Rust toolchain installation failed."
    exit 1
fi

# RPM packager (pure Rust, no rpmbuild dependency)
if ! command -v cargo-generate-rpm >/dev/null 2>&1; then
    echo "[!] Missing 'cargo-generate-rpm' - installing via cargo..."
    if ! cargo install cargo-generate-rpm --locked; then
        echo "✘ Could not install cargo-generate-rpm."
        exit 1
    fi
    hash -r
fi
if ! command -v cargo-generate-rpm >/dev/null 2>&1; then
    echo "✘ 'cargo-generate-rpm' is still missing after installation."
    exit 1
fi

echo -e "${GREEN}  ✓ Build & packaging prerequisites ready${NC}"

MODE="${1:-release}"
HOST_TARGET="$(rustc -vV | grep host | cut -d' ' -f2)"

if [ -n "${2:-}" ]; then
    TARGETS=("$2")
else
    TARGETS=("$HOST_TARGET")
    # Also ship arm64 packages unless we are already running on arm64.
    if [ "$HOST_TARGET" != "aarch64-unknown-linux-gnu" ]; then
        TARGETS+=("aarch64-unknown-linux-gnu")
    fi
fi

build_and_package() {
    local target="$1"
    local bin=""
    local dist_dir="$DIR/dist/$target"

    if command -v rustup >/dev/null 2>&1; then
        if ! rustup target list --installed 2>/dev/null | grep -qx "$target"; then
            echo "[!] Rust target '$target' not installed - adding..."
            rustup target add "$target"
        fi
    fi

    if [ "$target" = "$HOST_TARGET" ]; then
        if [ "$MODE" = "dev" ] || [ "$MODE" = "debug" ]; then
            echo -e "${YELLOW}⚡ Quick Building Desktop Client (Debug - $target)...${NC}"
            cargo build -p rust-rdp-vnc-desktop --target "$target"
            bin="$DIR/target/$target/debug/rust-rdp-vnc"
        else
            echo -e "${YELLOW}⚡ Quick Building Desktop Client (Release - $target)...${NC}"
            cargo build -p rust-rdp-vnc-desktop --release --target "$target"
            bin="$DIR/target/$target/release/rust-rdp-vnc"
        fi
    else
        # Cross-compile other architectures with Zig (no root/toolchain setup needed).
        ensure_zig
        ensure_cargo_zigbuild
        if [ "$MODE" = "dev" ] || [ "$MODE" = "debug" ]; then
            echo -e "${YELLOW}⚡ Cross Building Desktop Client (Debug - $target)...${NC}"
            RUSTFLAGS="${RUSTFLAGS:-} -A linker_messages" \
                cargo zigbuild -p rust-rdp-vnc-desktop --target "$target"
            bin="$DIR/target/$target/debug/rust-rdp-vnc"
        else
            echo -e "${YELLOW}⚡ Cross Building Desktop Client (Release - $target)...${NC}"
            RUSTFLAGS="${RUSTFLAGS:-} -A linker_messages" \
                cargo zigbuild -p rust-rdp-vnc-desktop --release --target "$target"
            bin="$DIR/target/$target/release/rust-rdp-vnc"
        fi
    fi

    if [ ! -f "$bin" ] && [ "$target" = "$HOST_TARGET" ]; then
        # Fallback path if cargo placed binary directly in target/{release,debug}
        if [ "$MODE" = "dev" ] || [ "$MODE" = "debug" ]; then
            bin="$DIR/target/debug/rust-rdp-vnc"
        else
            bin="$DIR/target/release/rust-rdp-vnc"
        fi
    fi

    if [ ! -f "$bin" ]; then
        echo -e "\n${RED}✘ Error: Binary not found at $bin${NC}"
        exit 1
    fi

    mkdir -p "$dist_dir"
    cp "$bin" "$dist_dir/"

    echo -e "\n${CYAN}📦 Packaging .deb, .rpm, and .tar.gz for $target...${NC}"
    python3 scripts/package_linux.py "$dist_dir" "$target"

    local missing=0
    local pattern
    for pattern in "*.deb" "*.rpm" "*.tar.gz"; do
        if ! compgen -G "$dist_dir/$pattern" >/dev/null; then
            echo -e "  ${RED}✘ Expected package not created: $pattern${NC}"
            missing=1
        fi
    done
    if [ "$missing" -ne 0 ]; then
        echo -e "\n${RED}${BOLD}✘ Packaging incomplete for $target - expected .deb, .rpm and .tar.gz.${NC}"
        exit 1
    fi
}

for target in "${TARGETS[@]}"; do
    build_and_package "$target"
done

echo -e "\n${GREEN}${BOLD}✔ Quick Build & Packaging Completed Successfully!${NC}"
echo -e "${CYAN}Output Packages:${NC}"
for target in "${TARGETS[@]}"; do
    echo -e "  ${BOLD}$DIR/dist/$target/${NC}"
    ls -lh "$DIR/dist/$target"/* 2>/dev/null || true
done

if [ "$MODE" = "dev" ] || [ "$MODE" = "debug" ]; then
    HOST_BIN="$DIR/target/$HOST_TARGET/debug/rust-rdp-vnc"
else
    HOST_BIN="$DIR/target/$HOST_TARGET/release/rust-rdp-vnc"
fi
echo -e "\n${CYAN}Run binary immediately:${NC}"
echo -e "  $HOST_BIN"
