#!/bin/bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}Installing Forge...${NC}"

# Detect architecture
ARCH=$(uname -m)
case $ARCH in
    x86_64 | x64 | amd64)
        ARCH="x86_64"
        ;;
    aarch64 | arm64)
        ARCH="aarch64"
        ;;
    *)
        echo -e "${RED}Unsupported architecture: $ARCH${NC}"
        echo -e "${YELLOW}Supported architectures: x86_64, aarch64${NC}"
        exit 1
        ;;
esac

# Check if running on Android
is_android() {
    # Check for Termux environment
    if [ -n "$PREFIX" ] && echo "$PREFIX" | grep -q "com.termux"; then
        return 0
    fi
    
    # Check for Android-specific environment variables
    if [ -n "$ANDROID_ROOT" ] || [ -n "$ANDROID_DATA" ]; then
        return 0
    fi
    
    # Check for Android-specific system properties
    if [ -f "/system/build.prop" ]; then
        return 0
    fi
    
    # Try getprop command (Android-specific)
    if command -v getprop >/dev/null 2>&1; then
        if getprop ro.build.version.release >/dev/null 2>&1; then
            return 0
        fi
    fi
    
    return 1
}

# Get glibc version and type
get_libc_info() {
    # Try ldd first
    local ldd_output=$(ldd --version 2>&1 | head -n 1 || true)
    
    # Check if this is musl libc
    if echo "$ldd_output" | grep -qiF "musl"; then
        echo "musl"
        return
    fi
    
    # Extract glibc version
    local version=$(echo "$ldd_output" | grep -oE '[0-9]+\.[0-9]+' | head -n 1)
    
    # If no version found from ldd, try getconf
    if [ -z "$version" ]; then
        if command -v getconf >/dev/null 2>&1; then
            local getconf_output=$(getconf GNU_LIBC_VERSION 2>/dev/null || true)
            version=$(echo "$getconf_output" | grep -oE '[0-9]+\.[0-9]+' | head -n 1)
        fi
    fi
    
    # If we have a version, check if it's sufficient (>= 2.39)
    if [ -n "$version" ]; then
        # Convert version to comparable number (e.g., 2.39 -> 239)
        local major=$(echo "$version" | cut -d. -f1)
        local minor=$(echo "$version" | cut -d. -f2)
        local version_num=$((major * 100 + minor))
        
        # Our binary requires glibc 2.39 or higher
        if [ "$version_num" -ge 239 ]; then
            echo "gnu"
        else
            echo "musl"
        fi
    else
        # Default to gnu if we can't determine version
        echo "gnu"
    fi
}

# Detect OS
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

# Check for Android first
if [ "$OS" = "linux" ] && is_android; then
    TARGET="$ARCH-linux-android"
    BINARY_NAME="forge"
    INSTALL_DIR="$PREFIX/bin"
    if [ -z "$INSTALL_DIR" ]; then
        INSTALL_DIR="$HOME/.local/bin"
    fi
    USE_SUDO=false
else
    case $OS in
        linux)
            # Check for FORCE_MUSL environment variable
            if [ "$FORCE_MUSL" = "1" ]; then
                LIBC_SUFFIX="-musl"
            else
                # Detect libc type and version
                LIBC_TYPE=$(get_libc_info)
                LIBC_SUFFIX="-$LIBC_TYPE"
            fi
            TARGET="$ARCH-unknown-linux$LIBC_SUFFIX"
            BINARY_NAME="forge"
            # Prefer user-local directory to avoid sudo
            INSTALL_DIR="$HOME/.local/bin"
            USE_SUDO=false
            ;;
        darwin)
            TARGET="$ARCH-apple-darwin"
            BINARY_NAME="forge"
            # Prefer user-local directory to avoid sudo
            INSTALL_DIR="$HOME/.local/bin"
            USE_SUDO=false
            ;;
        msys* | mingw* | cygwin* | windows*)
            TARGET="$ARCH-pc-windows-msvc"
            BINARY_NAME="forge.exe"
            # Windows install to user's local bin or AppData
            if [ -n "$LOCALAPPDATA" ]; then
                INSTALL_DIR="$LOCALAPPDATA/Programs/Forge"
            else
                INSTALL_DIR="$HOME/.local/bin"
            fi
            USE_SUDO=false
            ;;
        *)
            echo -e "${RED}Unsupported operating system: $OS${NC}"
            echo -e "${YELLOW}Supported operating systems: Linux, macOS (Darwin), Windows${NC}"
            echo -e "${BLUE}For installation instructions, visit:${NC}"
            echo -e "${BLUE}https://github.com/antinomyhq/forge#installation${NC}"
            exit 1
            ;;
    esac
fi

echo -e "${BLUE}Detected platform: $TARGET${NC}"

# Allow optional version argument, defaulting to "latest"
VERSION="${1:-latest}"

# Construct download URL
DOWNLOAD_URL="https://release-download.tailcall.workers.dev/download/$VERSION/forge-$TARGET"

# Create temp directory
TMP_DIR=$(mktemp -d)
TEMP_BINARY="$TMP_DIR/$BINARY_NAME"

# Download Forge
echo -e "${BLUE}Downloading Forge from $DOWNLOAD_URL...${NC}"
if ! curl -fL "$DOWNLOAD_URL" -o "$TEMP_BINARY"; then
    echo -e "${RED}Failed to download Forge.${NC}"
    echo -e "${YELLOW}Please check:${NC}"
    echo -e "  - Your internet connection"
    echo -e "  - The version '$VERSION' exists"
    echo -e "  - The target '$TARGET' is supported"
    rm -rf "$TMP_DIR"
    exit 1
fi

# Create install directory if it doesn't exist
if [ ! -d "$INSTALL_DIR" ]; then
    echo -e "${BLUE}Creating installation directory: $INSTALL_DIR${NC}"
    if [ "$USE_SUDO" = true ]; then
        sudo mkdir -p "$INSTALL_DIR"
    else
        mkdir -p "$INSTALL_DIR"
    fi
fi

# Install
INSTALL_PATH="$INSTALL_DIR/$BINARY_NAME"
echo -e "${BLUE}Installing to $INSTALL_PATH...${NC}"
if [ "$USE_SUDO" = true ]; then
    sudo mv "$TEMP_BINARY" "$INSTALL_PATH"
    sudo chmod +x "$INSTALL_PATH"
else
    mv "$TEMP_BINARY" "$INSTALL_PATH"
    chmod +x "$INSTALL_PATH"
fi
rm -rf "$TMP_DIR"

# Add to PATH if necessary (for Windows or non-standard install locations)
if [ "$OS" = "windows" ] || [ "$OS" = "msys" ] || [ "$OS" = "mingw" ] || [ "$OS" = "cygwin" ]; then
    if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
        echo -e "${YELLOW}Note: You may need to add $INSTALL_DIR to your PATH${NC}"
    fi
fi

# Verify installation
echo ""
if command -v forge >/dev/null 2>&1; then
    echo -e "${GREEN}✓ Forge has been successfully installed!${NC}"
    forge --version 2>/dev/null || true
    echo -e "${BLUE}Run 'forge' to get started.${NC}"
else
    echo -e "${GREEN}✓ Forge has been installed to $INSTALL_PATH${NC}"
    echo ""
    echo -e "${YELLOW}The 'forge' command is not in your PATH yet.${NC}"
    
    # Check if the install directory is in PATH
    if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
        echo -e "${BLUE}Add it to your PATH by running:${NC}"
        
        # Provide shell-specific instructions
        if [ -n "$ZSH_VERSION" ]; then
            echo -e "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.zshrc"
            echo -e "  source ~/.zshrc"
        elif [ -n "$BASH_VERSION" ]; then
            echo -e "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc"
            echo -e "  source ~/.bashrc"
        else
            echo -e "  export PATH=\"$INSTALL_DIR:\$PATH\""
        fi
    else
        echo -e "${BLUE}Restart your shell or run:${NC}"
        echo -e "  source ~/.$(basename $SHELL)rc"
    fi
fi
