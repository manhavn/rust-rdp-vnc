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

MODE="${1:-release}"
HOST_TARGET="$(rustc -vV | grep host | cut -d' ' -f2)"
TARGET="${2:-$HOST_TARGET}"

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

echo -e "\n${GREEN}${BOLD}✔ Quick Build & Packaging Completed Successfully!${NC}"
echo -e "${CYAN}Output Packages in:${NC} ${BOLD}$DIST_TARGET_DIR/${NC}"
ls -lh "$DIST_TARGET_DIR"/* 2>/dev/null || true

echo -e "\n${CYAN}Run binary immediately:${NC}"
echo -e "  $BIN"
