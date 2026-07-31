#!/bin/bash
# Exit on error
set -e

# ANSI escape codes for coloring
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color
BOLD='\033[1m'

echo -e "${CYAN}${BOLD}==================================================${NC}"
echo -e "${CYAN}${BOLD}       ANTIGRAVITY RDP CLIENT - BUILD IOS         ${NC}"
echo -e "${CYAN}${BOLD}==================================================${NC}"

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
MODE="${1:-release}"
TARGET="${2:-aarch64-apple-ios}"

if [ "$MODE" = "dev" ] || [ "$MODE" = "debug" ]; then
    BUILD_FLAG=""
    OUTPUT_DIR="debug"
else
    BUILD_FLAG="--release"
    OUTPUT_DIR="release"
fi

echo -e "${YELLOW}Building Rust core library for iOS ($TARGET, $MODE)...${NC}"

# Check if target is installed, attempt rustup target add if missing
if command -v rustup &> /dev/null; then
    if ! rustup target list | grep -q "$TARGET (installed)"; then
        echo -e "${YELLOW}Target $TARGET not installed. Installing...${NC}"
        rustup target add "$TARGET" || true
    fi
fi

echo -e "\n${CYAN}Running cargo build for target: $TARGET...${NC}"
cd "$DIR"
cargo build -p rust_rdp --target "$TARGET" $BUILD_FLAG --features ios

LIB_PATH="$DIR/target/$TARGET/$OUTPUT_DIR/librust_rdp.a"

if [ -f "$LIB_PATH" ]; then
    echo -e "\n${GREEN}${BOLD}✔ Rust iOS Library Built Successfully!${NC}"
    echo -e "${GREEN}Static Library:${NC} ${BOLD}$LIB_PATH${NC}"
    ls -lh "$LIB_PATH"
else
    echo -e "\n${RED}✘ Error: Static library file was not found at $LIB_PATH${NC}"
    exit 1
fi

# If running on macOS with xcodebuild available, build Xcode project
if command -v xcodebuild &> /dev/null; then
    echo -e "\n${CYAN}Building Xcode project (RustRdpVnc)...${NC}"
    cd "$DIR/ios"
    xcodebuild -project RustRdpVnc.xcodeproj -scheme RustRdpVnc -sdk iphoneos -configuration Release CODE_SIGN_IDENTITY="" CODE_SIGNING_REQUIRED=NO build
    echo -e "\n${GREEN}${BOLD}✔ Xcode iOS Build Completed Successfully!${NC}"
else
    echo -e "\n${YELLOW}Note: xcodebuild is only available on macOS. Rust static library for iOS ($TARGET) has been compiled successfully.${NC}"
    echo -e "${YELLOW}To build the final iOS .app / .ipa, open the project in Xcode on macOS:${NC}"
    echo -e "  ${BOLD}open $DIR/ios/RustRdpVnc.xcodeproj${NC}"
fi

echo -e "\n${GREEN}${BOLD}✔ iOS Build Pipeline Finished!${NC}"
