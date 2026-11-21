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
TARGET="${2:-$HOST_TARGET}"

if command -v rustup >/dev/null 2>&1; then
    if ! rustup target list --installed 2>/dev/null | grep -qx "$TARGET"; then
        echo "[!] Rust target '$TARGET' not installed - adding..."
        rustup target add "$TARGET"
    fi
fi

if [ "$MODE" = "dev" ] || [ "$MODE" = "debug" ]; then
    echo -e "${YELLOW}⚡ Quick Building Desktop Client (Debug - $TARGET)...${NC}"
    cargo build -p rust-rdp-vnc-desktop --target "$TARGET"
    BIN="$DIR/target/$TARGET/debug/rust-rdp-vnc"
else
    echo -e "${YELLOW}⚡ Quick Building Desktop Client (Release - $TARGET)...${NC}"
    cargo build -p rust-rdp-vnc-desktop --release --target "$TARGET"
    BIN="$DIR/target/$TARGET/release/rust-rdp-vnc"
fi

if [ ! -f "$BIN" ]; then
    # Fallback path if cargo placed binary directly in target/release
    if [ "$MODE" = "dev" ] || [ "$MODE" = "debug" ]; then
        BIN="$DIR/target/debug/rust-rdp-vnc"
    else
        BIN="$DIR/target/release/rust-rdp-vnc"
    fi
fi

if [ ! -f "$BIN" ]; then
    echo -e "\n${RED}✘ Error: Binary not found at $BIN${NC}"
    exit 1
fi

DIST_TARGET_DIR="$DIR/dist/$TARGET"
mkdir -p "$DIST_TARGET_DIR"
cp "$BIN" "$DIST_TARGET_DIR/"

echo -e "\n${CYAN}📦 Packaging .deb, .rpm, and .tar.gz...${NC}"
python3 scripts/package_linux.py "$DIST_TARGET_DIR" "$TARGET"

MISSING_PACKAGES=0
for pattern in "*.deb" "*.rpm" "*.tar.gz"; do
    if ! compgen -G "$DIST_TARGET_DIR/$pattern" >/dev/null; then
        echo -e "  ${RED}✘ Expected package not created: $pattern${NC}"
        MISSING_PACKAGES=1
    fi
done
if [ "$MISSING_PACKAGES" -ne 0 ]; then
    echo -e "\n${RED}${BOLD}✘ Packaging incomplete - expected .deb, .rpm and .tar.gz outputs.${NC}"
    exit 1
fi

echo -e "\n${GREEN}${BOLD}✔ Quick Build & Packaging Completed Successfully!${NC}"
echo -e "${CYAN}Output Packages in:${NC} ${BOLD}$DIST_TARGET_DIR/${NC}"
ls -lh "$DIST_TARGET_DIR"/* 2>/dev/null || true

echo -e "\n${CYAN}Run binary immediately:${NC}"
echo -e "  $BIN"
