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
echo -e "${CYAN}${BOLD}       ANTIGRAVITY RDP CLIENT - BUILD DEV         ${NC}"
echo -e "${CYAN}${BOLD}==================================================${NC}"
echo -e "${YELLOW}Building Rust core library (debug) and Android app (debug)...${NC}"

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

# Set environment defaults if java/gradle not in PATH
if [ -z "$JAVA_HOME" ] && [ -d "/home/dev/tools/jdk-17.0.10+7" ]; then
    export JAVA_HOME="/home/dev/tools/jdk-17.0.10+7"
    export PATH="$JAVA_HOME/bin:$PATH"
fi
if ! command -v gradle &> /dev/null && [ -d "/home/dev/tools/gradle-8.7/bin" ]; then
    export PATH="/home/dev/tools/gradle-8.7/bin:$PATH"
fi

# Run gradle debug build, passing rustDebug property
echo -e "\n${CYAN}Running gradle build task with -PrustDebug...${NC}"
cd "$DIR/android"
gradle assembleDebug -PrustDebug

# Print completion info
APK_PATH="$DIR/android/app/build/outputs/apk/debug/app-debug.apk"
if [ -f "$APK_PATH" ]; then
    echo -e "\n${GREEN}${BOLD}✔ Build Dev Completed Successfully!${NC}"
    echo -e "${GREEN}Generated APK:${NC} ${BOLD}$APK_PATH${NC}"
    ls -lh "$APK_PATH"
else
    echo -e "\n${RED}✘ Error: APK file was not found at $APK_PATH${NC}"
    exit 1
fi
