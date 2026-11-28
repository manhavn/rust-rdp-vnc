#!/usr/bin/env bash
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
echo -e "${CYAN}${BOLD}     ANTIGRAVITY RDP CLIENT - BUILD RELEASE       ${NC}"
echo -e "${CYAN}${BOLD}==================================================${NC}"
echo -e "${YELLOW}Checking prerequisites & building Rust core (release) + Android APK (release)...${NC}"

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"

extract_zip() {
    local zip_file="$1"
    local dest_dir="$2"
    mkdir -p "$dest_dir"
    if command -v unzip &> /dev/null; then
        unzip -q -o "$zip_file" -d "$dest_dir"
    else
        python3 -m zipfile -e "$zip_file" "$dest_dir"
    fi
}

# 1. Ensure Java / JAVA_HOME
if [ -z "$JAVA_HOME" ] || [ ! -x "$JAVA_HOME/bin/java" ]; then
    for candidate in \
        "/home/dev/tools/jdk-17.0.10+7" \
        "$HOME/.local/opt/jdk-17" \
        "$HOME/.local/share/mise/installs/java/17"* \
        "$HOME/.local/share/mise/installs/java/21"* \
        "$HOME/.local/share/mise/installs/java/"* \
        /usr/lib/jvm/java-17* \
        /usr/lib/jvm/java-21* \
        /usr/lib/jvm/*; do
        if [ -x "$candidate/bin/java" ]; then
            export JAVA_HOME="$candidate"
            break
        fi
    done
fi

if [ -z "$JAVA_HOME" ] && command -v java &> /dev/null; then
    JAVA_BIN="$(readlink -f "$(command -v java)")"
    JAVA_CANDIDATE="$(dirname "$(dirname "$JAVA_BIN")")"
    if [ -x "$JAVA_CANDIDATE/bin/java" ]; then
        export JAVA_HOME="$JAVA_CANDIDATE"
    fi
fi

if [ -z "$JAVA_HOME" ] || [ ! -x "$JAVA_HOME/bin/java" ]; then
    echo -e "${YELLOW}[!] JDK not found. Downloading OpenJDK 17...${NC}"
    JDK_DIR="$HOME/.local/opt/jdk-17"
    JDK_TAR="/tmp/openjdk-17-linux-x64.tar.gz"
    mkdir -p "$JDK_DIR"
    curl -fSL "https://api.adoptium.net/v3/binary/latest/17/ga/linux/x64/jdk/hotspot/normal/eclipse" -o "$JDK_TAR"
    tar -xzf "$JDK_TAR" --strip-components=1 -C "$JDK_DIR"
    rm -f "$JDK_TAR"
    export JAVA_HOME="$JDK_DIR"
fi
export PATH="$JAVA_HOME/bin:$PATH"
echo -e "${GREEN}  ✓ JAVA_HOME:${NC} $JAVA_HOME"

# 2. Ensure Gradle
if ! command -v gradle &> /dev/null; then
    for candidate_bin in \
        "/home/dev/tools/gradle-8.7/bin" \
        "$HOME/.local/opt/gradle-"*/bin; do
        if [ -x "$candidate_bin/gradle" ]; then
            export PATH="$candidate_bin:$PATH"
            break
        fi
    done
fi

if ! command -v gradle &> /dev/null; then
    WRAPPER_GRADLE="$(find "$HOME/.gradle/wrapper/dists" -maxdepth 5 -type f -name "gradle" 2>/dev/null | sort -V | tail -n 1 || true)"
    if [ -n "$WRAPPER_GRADLE" ] && [ -x "$WRAPPER_GRADLE" ]; then
        export PATH="$(dirname "$WRAPPER_GRADLE"):$PATH"
    fi
fi

if ! command -v gradle &> /dev/null; then
    GRADLE_VERSION="8.7"
    echo -e "${YELLOW}[!] Gradle not found. Downloading Gradle $GRADLE_VERSION...${NC}"
    GRADLE_ZIP="/tmp/gradle-${GRADLE_VERSION}-bin.zip"
    curl -fSL "https://services.gradle.org/distributions/gradle-${GRADLE_VERSION}-bin.zip" -o "$GRADLE_ZIP"
    extract_zip "$GRADLE_ZIP" "$HOME/.local/opt"
    rm -f "$GRADLE_ZIP"
    chmod +x "$HOME/.local/opt/gradle-${GRADLE_VERSION}/bin/gradle"
    export PATH="$HOME/.local/opt/gradle-${GRADLE_VERSION}/bin:$PATH"
fi
echo -e "${GREEN}  ✓ Gradle:${NC} $(command -v gradle)"

# 3. Ensure Rust, Android targets, and cargo-ndk
if ! command -v cargo &> /dev/null || ! command -v rustup &> /dev/null; then
    echo -e "${YELLOW}[!] Rust/Cargo not found. Installing rustup...${NC}"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    if [ -f "$HOME/.cargo/env" ]; then
        # shellcheck disable=SC1091
        source "$HOME/.cargo/env"
    fi
fi

INSTALLED_TARGETS="$(rustup target list --installed)"
for target in "aarch64-linux-android" "x86_64-linux-android"; do
    if ! echo "$INSTALLED_TARGETS" | grep -qx "$target"; then
        echo -e "${YELLOW}[!] Installing missing Rust target: $target...${NC}"
        rustup target add "$target"
    fi
done
echo -e "${GREEN}  ✓ Rust Android targets:${NC} aarch64-linux-android, x86_64-linux-android"

if ! command -v cargo-ndk &> /dev/null; then
    echo -e "${YELLOW}[!] cargo-ndk not found. Installing cargo-ndk...${NC}"
    cargo install cargo-ndk --locked
fi
echo -e "${GREEN}  ✓ cargo-ndk:${NC} $(cargo ndk --version)"

# 4. Ensure Android SDK, cmdline-tools, platform/build-tools, and NDK
if [ -z "$ANDROID_HOME" ] || [ ! -d "$ANDROID_HOME" ]; then
    if [ -n "$ANDROID_SDK_ROOT" ] && [ -d "$ANDROID_SDK_ROOT" ]; then
        export ANDROID_HOME="$ANDROID_SDK_ROOT"
    else
        for candidate in \
            "$HOME/Android/Sdk" \
            "/home/dev/Android/Sdk" \
            "/home/dev2/Android/android-sdk" \
            "/home/dev/Android/android-sdk"; do
            if [ -d "$candidate" ]; then
                export ANDROID_HOME="$candidate"
                break
            fi
        done
    fi
fi

if [ -z "$ANDROID_HOME" ]; then
    export ANDROID_HOME="$HOME/Android/Sdk"
fi
mkdir -p "$ANDROID_HOME"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
echo "sdk.dir=$ANDROID_HOME" > "$DIR/android/local.properties"
echo -e "${GREEN}  ✓ ANDROID_HOME:${NC} $ANDROID_HOME"

find_sdkmanager() {
    if [ -x "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" ]; then
        echo "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
        return 0
    fi
    local found
    found="$(find "$ANDROID_HOME/cmdline-tools" -maxdepth 3 -type f -name "sdkmanager" 2>/dev/null | head -n 1 || true)"
    if [ -n "$found" ] && [ -x "$found" ]; then
        echo "$found"
        return 0
    fi
    if command -v sdkmanager &> /dev/null; then
        command -v sdkmanager
        return 0
    fi
    return 1
}

SDKMANAGER=""
ensure_sdkmanager() {
    if [ -n "$SDKMANAGER" ] && [ -x "$SDKMANAGER" ]; then
        return 0
    fi
    SDKMANAGER="$(find_sdkmanager || true)"
    if [ -z "$SDKMANAGER" ]; then
        echo -e "${YELLOW}[!] Android cmdline-tools not found. Downloading Android SDK command-line tools...${NC}"
        local tools_zip="/tmp/commandlinetools-linux-latest.zip"
        local tmp_dir="/tmp/android-cmdline-tools-extract"
        rm -rf "$tmp_dir"
        curl -fSL "https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip" -o "$tools_zip"
        extract_zip "$tools_zip" "$tmp_dir"
        mkdir -p "$ANDROID_HOME/cmdline-tools"
        rm -rf "$ANDROID_HOME/cmdline-tools/latest"
        mv "$tmp_dir/cmdline-tools" "$ANDROID_HOME/cmdline-tools/latest"
        rm -rf "$tmp_dir" "$tools_zip"
        chmod +x "$ANDROID_HOME/cmdline-tools/latest/bin/"*
        SDKMANAGER="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
    fi
    { yes || true; } | "$SDKMANAGER" --sdk_root="$ANDROID_HOME" --licenses >/dev/null 2>&1 || true
}

SDK_PACKAGES=()
if [ ! -d "$ANDROID_HOME/platforms/android-35" ]; then
    SDK_PACKAGES+=("platforms;android-35")
fi
if [ ! -d "$ANDROID_HOME/platform-tools" ]; then
    SDK_PACKAGES+=("platform-tools")
fi
if [ ! -d "$ANDROID_HOME/build-tools/34.0.0" ] && ! compgen -G "$ANDROID_HOME/build-tools/34.*" > /dev/null && ! compgen -G "$ANDROID_HOME/build-tools/35.*" > /dev/null; then
    SDK_PACKAGES+=("build-tools;34.0.0")
fi

if [ ${#SDK_PACKAGES[@]} -gt 0 ]; then
    ensure_sdkmanager
    echo -e "${YELLOW}[!] Installing missing Android SDK packages: ${SDK_PACKAGES[*]}...${NC}"
    "$SDKMANAGER" --sdk_root="$ANDROID_HOME" "${SDK_PACKAGES[@]}"
fi

# Detect or install Android NDK
is_valid_ndk() {
    local ndk_path="$1"
    [ -n "$ndk_path" ] && \
    [ -f "$ndk_path/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so" ] && \
    [ -f "$ndk_path/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/x86_64-linux-android/libc++_shared.so" ]
}

if ! is_valid_ndk "$ANDROID_NDK_HOME"; then
    ANDROID_NDK_HOME=""
    if [ -d "$ANDROID_HOME/ndk" ]; then
        while IFS= read -r ndk_candidate; do
            if is_valid_ndk "$ndk_candidate"; then
                ANDROID_NDK_HOME="$ndk_candidate"
            fi
        done < <(find "$ANDROID_HOME/ndk" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort -V)
    fi
fi

if ! is_valid_ndk "$ANDROID_NDK_HOME"; then
    ensure_sdkmanager
    DEFAULT_NDK_VERSION="25.2.9519653"
    echo -e "${YELLOW}[!] Valid Android NDK not found. Installing ndk;$DEFAULT_NDK_VERSION...${NC}"
    "$SDKMANAGER" --sdk_root="$ANDROID_HOME" "ndk;$DEFAULT_NDK_VERSION"
    ANDROID_NDK_HOME="$ANDROID_HOME/ndk/$DEFAULT_NDK_VERSION"
fi

export ANDROID_NDK_HOME
echo -e "${GREEN}  ✓ ANDROID_NDK_HOME:${NC} $ANDROID_NDK_HOME"

# 5. Run gradle release build
echo -e "\n${CYAN}Running gradle assembleRelease...${NC}"
cd "$DIR/android"
gradle assembleRelease --no-daemon

# 6. Print completion info
APK_PATH="$DIR/android/app/build/outputs/apk/release/app-release-unsigned.apk"
APK_PATH_SIGNED="$DIR/android/app/build/outputs/apk/release/app-release.apk"

if [ -f "$APK_PATH_SIGNED" ]; then
    echo -e "\n${GREEN}${BOLD}✔ Build Release Completed Successfully!${NC}"
    echo -e "${GREEN}Generated APK:${NC} ${BOLD}$APK_PATH_SIGNED${NC}"
    ls -lh "$APK_PATH_SIGNED"
elif [ -f "$APK_PATH" ]; then
    echo -e "\n${GREEN}${BOLD}✔ Build Release Completed Successfully! (Unsigned)${NC}"
    echo -e "${GREEN}Generated APK:${NC} ${BOLD}$APK_PATH${NC}"
    ls -lh "$APK_PATH"
else
    echo -e "\n${RED}✘ Error: Release APK file was not found in outputs directory.${NC}"
    exit 1
fi
