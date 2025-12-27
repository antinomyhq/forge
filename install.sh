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

# Detect OS
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
case $OS in
    linux)
        # Detect libc for Linux
        LIBC_INFO=$(ldd --version 2>&1 | head -n 1 || true)
        if echo "$LIBC_INFO" | grep -qiF "musl"; then
            LIBC_SUFFIX="-musl"
        else
            LIBC_SUFFIX="-gnu"
        fi
        TARGET="$ARCH-unknown-linux$LIBC_SUFFIX"
        BINARY_NAME="forge"
        INSTALL_DIR="/usr/local/bin"
        USE_SUDO=true
        ;;
    darwin)
        TARGET="$ARCH-apple-darwin"
        BINARY_NAME="forge"
        INSTALL_DIR="/usr/local/bin"
        # Check if we need sudo for /usr/local/bin
        if [ -w "$INSTALL_DIR" ]; then
            USE_SUDO=false
        else
            USE_SUDO=true
        fi
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
if command -v forge >/dev/null 2>&1; then
    echo -e "${GREEN}Forge has been successfully installed!${NC}"
    forge --version 2>/dev/null || true
    echo -e "${BLUE}You can now run 'forge' to get started.${NC}"
else
    echo -e "${YELLOW}Forge has been installed to $INSTALL_PATH${NC}"
    echo -e "${YELLOW}If 'forge' command is not found, ensure $INSTALL_DIR is in your PATH${NC}"
    if [ "$USE_SUDO" = false ]; then
        echo -e "${BLUE}You may need to restart your shell or run:${NC}"
        echo -e "  export PATH=\"\$PATH:$INSTALL_DIR\""
    fi
fi
