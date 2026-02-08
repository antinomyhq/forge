#!/bin/bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}Installing Forge...${NC}"

# Check for required dependencies
DOWNLOADER=""
if command -v curl >/dev/null 2>&1; then
    DOWNLOADER="curl"
elif command -v wget >/dev/null 2>&1; then
    DOWNLOADER="wget"
else
    echo -e "${RED}Error: Either curl or wget is required but neither is installed${NC}" >&2
    exit 1
fi

# Download function that works with both curl and wget
download_file() {
    local url="$1"
    local output="$2"
    
    if [ "$DOWNLOADER" = "curl" ]; then
        curl -fsSL -o "$output" "$url"
    elif [ "$DOWNLOADER" = "wget" ]; then
        wget -q -O "$output" "$url"
    else
        return 1
    fi
}

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
    # Check for musl library files first (faster and more reliable)
    if [ -f "/lib/libc.musl-x86_64.so.1" ] || [ -f "/lib/libc.musl-aarch64.so.1" ]; then
        echo "musl"
        return
    fi
    
    # Find ls binary dynamically (more portable)
    local ls_binary=$(command -v ls 2>/dev/null || echo "/bin/ls")
    
    # Check if ldd reports musl (if ldd exists)
    if command -v ldd >/dev/null 2>&1; then
        if ldd "$ls_binary" 2>&1 | grep -q musl; then
            echo "musl"
            return
        fi
    fi
    
    # Try ldd for glibc version (if ldd exists)
    if command -v ldd >/dev/null 2>&1; then
        local ldd_output=$(ldd --version 2>&1 | head -n 1 || true)
        
        # Double-check it's not musl
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
                return
            else
                echo "musl"
                return
            fi
        fi
    fi
    
    # If ldd doesn't exist or we couldn't determine, default to gnu
    # (most common on standard Linux distributions)
    echo "gnu"
}

# Detect OS
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

# Check for Android first
if [ "$OS" = "linux" ] && is_android; then
    TARGET="$ARCH-linux-android"
    BINARY_NAME="forge"
    if [ -z "$PREFIX" ]; then
        INSTALL_DIR="$HOME/.local/bin"
    else
        INSTALL_DIR="$PREFIX/bin"
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
# If version is "latest", fetch the actual latest version from GitHub
if [ "$VERSION" = "latest" ]; then
    # Fetch latest release tag from GitHub API
    if [ "$DOWNLOADER" = "curl" ]; then
        LATEST_VERSION=$(curl -fsSL https://api.github.com/repos/antinomyhq/forge/releases/latest | grep '"tag_name"' | head -n 1 | cut -d '"' -f 4)
    elif [ "$DOWNLOADER" = "wget" ]; then
        LATEST_VERSION=$(wget -qO- https://api.github.com/repos/antinomyhq/forge/releases/latest | grep '"tag_name"' | head -n 1 | cut -d '"' -f 4)
    fi
    
    # Fallback if we couldn't get the latest version
    if [ -z "$LATEST_VERSION" ]; then
        echo -e "${RED}Failed to fetch latest version from GitHub${NC}" >&2
        exit 1
    fi
    VERSION="$LATEST_VERSION"
fi

# Use direct GitHub releases URL
DOWNLOAD_URL="https://github.com/antinomyhq/forge/releases/download/$VERSION/forge-$TARGET"

# Create temp directory
TMP_DIR=$(mktemp -d)
TEMP_BINARY="$TMP_DIR/$BINARY_NAME"

# Download Forge
echo -e "${BLUE}Downloading Forge from $DOWNLOAD_URL...${NC}"
if ! download_file "$DOWNLOAD_URL" "$TEMP_BINARY"; then
    echo -e "${RED}Failed to download Forge.${NC}" >&2
    echo -e "${YELLOW}Please check:${NC}" >&2
    echo -e "  - Your internet connection" >&2
    echo -e "  - The version '$VERSION' exists" >&2
    echo -e "  - The target '$TARGET' is supported" >&2
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
            echo -e "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.zshrc"
            echo -e "  source ~/.zshrc"
        elif [ -n "$BASH_VERSION" ]; then
            echo -e "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.bashrc"
            echo -e "  source ~/.bashrc"
        elif [ -n "$FISH_VERSION" ]; then
            echo -e "  fish_add_path $INSTALL_DIR"
        else
            echo -e "  export PATH=\"$INSTALL_DIR:\$PATH\""
        fi
    else
        echo -e "${BLUE}Restart your shell or run:${NC}"
        
        # Detect shell and provide appropriate source command
        local shell_name=$(basename "${SHELL:-bash}")
        case "$shell_name" in
            zsh)
                echo -e "  source ~/.zshrc"
                ;;
            bash)
                echo -e "  source ~/.bashrc"
                ;;
            fish)
                echo -e "  Restart your terminal (fish doesn't need source)"
                ;;
            *)
                echo -e "  Restart your terminal"
                ;;
        esac
    fi
fi
