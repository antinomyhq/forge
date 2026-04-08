#!/bin/sh
# NOTE: POSIX sh compatibility
# Users are instructed to install ForgeCode by piping this script to 'sh':
#
#   curl -fsSL https://raw.githubusercontent.com/Zetkolink/forgecode/main/scripts/install.sh | sh
#
# When a script is piped to 'sh', the shebang line above is ignored and the
# system shell (often dash on Ubuntu/WSL) is used. Dash does not support
# bash-specific syntax such as [[ ]] or =~.
#
# This script is therefore written to be fully POSIX sh compatible. Do NOT
# introduce bash-specific syntax (e.g. [[ ]], =~, local with arrays, echo -e,
# or process substitution). Use [ ], grep, and printf instead.

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

printf "${BLUE}Installing Forge and dependencies (fzf, bat, fd)...${NC}\n"

# Check for required dependencies
DOWNLOADER=""
if command -v curl > /dev/null 2>&1; then
  DOWNLOADER="curl"
elif command -v wget > /dev/null 2>&1; then
  DOWNLOADER="wget"
else
  printf "${RED}Error: Either curl or wget is required but neither is installed${NC}\n" >&2
  exit 1
fi

# Download function that works with both curl and wget
download_file() {
  download_url="$1"
  download_output="$2"

  if [ "$DOWNLOADER" = "curl" ]; then
    # First try default transport
    if curl -fsSL -o "$download_output" "$download_url"; then
      return 0
    fi

    # Fallback for intermittent HTTP/2 issues on some networks
    sleep 1
    curl -fsSL --http1.1 -o "$download_output" "$download_url"
  elif [ "$DOWNLOADER" = "wget" ]; then
    wget -q -O "$download_output" "$download_url"
  else
    return 1
  fi
}

# Function to check if a tool is already installed
check_tool_installed() {
  tool_name="$1"
  if command -v "$tool_name" > /dev/null 2>&1; then
    printf "${GREEN}✓ %s is already installed${NC}\n" "$tool_name"
    return 0
  fi
  return 1
}

# Function to get latest release version from GitHub
get_latest_version() {
  repo="$1"
  if [ "$DOWNLOADER" = "curl" ]; then
    curl -fsSL "https://api.github.com/repos/$repo/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
  else
    wget -qO- "https://api.github.com/repos/$repo/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
  fi
}

# Compare two semantic versions: returns 0 (true) if v1 < v2, 1 otherwise.
# Strips a leading 'v' and any build-metadata/pre-release suffix, then splits
# on '.' using IFS — zero subshells, zero external processes.
version_less_than() {
  # Strip leading 'v' and any suffix starting with '+' or '-'
  _v1="${1#v}"; _v1="${_v1%%+*}"; _v1="${_v1%%-*}"
  _v2="${2#v}"; _v2="${_v2%%+*}"; _v2="${_v2%%-*}"

  # Split on '.' via IFS without spawning a subshell
  IFS=. read -r _v1_major _v1_minor _v1_patch <<EOF
$_v1
EOF
  IFS=. read -r _v2_major _v2_minor _v2_patch <<EOF
$_v2
EOF

  # Default missing components to 0
  _v1_major=${_v1_major:-0}; _v1_minor=${_v1_minor:-0}; _v1_patch=${_v1_patch:-0}
  _v2_major=${_v2_major:-0}; _v2_minor=${_v2_minor:-0}; _v2_patch=${_v2_patch:-0}

  if   [ "$_v1_major" -lt "$_v2_major" ]; then return 0
  elif [ "$_v1_major" -gt "$_v2_major" ]; then return 1
  elif [ "$_v1_minor" -lt "$_v2_minor" ]; then return 0
  elif [ "$_v1_minor" -gt "$_v2_minor" ]; then return 1
  elif [ "$_v1_patch" -lt "$_v2_patch" ]; then return 0
  else return 1
  fi
}

# Return the installed fzf version (first word of "fzf --version" output).
# If a path is provided, inspect that specific binary instead of PATH.
get_fzf_version() {
  fzf_cmd="${1:-}"
  if [ -n "$fzf_cmd" ] && [ -x "$fzf_cmd" ]; then
    "$fzf_cmd" --version 2> /dev/null | cut -d' ' -f1
  elif command -v fzf > /dev/null 2>&1; then
    fzf --version 2> /dev/null | cut -d' ' -f1
  fi
}

# Prepend a directory to PATH once.
prepend_to_path() {
  path_dir="$1"
  case ":$PATH:" in
    *":$path_dir:"*) ;;
    *) export PATH="$path_dir:$PATH" ;;
  esac
}

# Persist PATH precedence for future bash/zsh shells so user-local binaries win.
ensure_install_dir_shell_path() {
  export_line="export PATH=\"$INSTALL_DIR:\$PATH\""
  marker="# Added by ForgeCode installer"

  for rc_file in "$HOME/.bashrc" "$HOME/.zshrc"; do
    temp_rc=$(mktemp)
    if [ -f "$rc_file" ]; then
      awk -v marker="$marker" -v line="$export_line" '$0 != marker && $0 != line' "$rc_file" > "$temp_rc"
      {
        printf '%s\n' "$marker"
        printf '%s\n' "$export_line"
        cat "$temp_rc"
      } > "$temp_rc.new"
      mv "$temp_rc.new" "$rc_file"
    else
      {
        printf '%s\n' "$marker"
        printf '%s\n' "$export_line"
      } > "$rc_file"
    fi
    rm -f "$temp_rc"
  done
}

# Function to install fzf
install_fzf() {
  existing_version=$(get_fzf_version)

  if echo "$OS" | grep -qE 'msys|mingw|cygwin|windows'; then
    fzf_binary="fzf.exe"
  else
    fzf_binary="fzf"
  fi

  managed_fzf_path="$INSTALL_DIR/$fzf_binary"
  managed_version=$(get_fzf_version "$managed_fzf_path")

  if [ -n "$managed_version" ] && ! version_less_than "$managed_version" "0.48.0"; then
    prepend_to_path "$INSTALL_DIR"
    printf "${GREEN}✓ fzf %s is already installed and compatible${NC}\n" "$managed_version"
    return 0
  fi

  if [ -n "$existing_version" ] && ! version_less_than "$existing_version" "0.48.0"; then
    printf "${GREEN}✓ fzf %s is already installed and compatible${NC}\n" "$existing_version"
    return 0
  fi

  if [ -n "$existing_version" ]; then
    printf "${YELLOW}fzf %s is installed but has a known bug; ForgeCode requires >= 0.48.0. Installing a newer user-local binary...${NC}\n" "$existing_version"
  else
    printf "${BLUE}Installing fzf...${NC}\n"
  fi

  fzf_version=$(get_latest_version "junegunn/fzf")
  if [ -z "$fzf_version" ]; then
    printf "${YELLOW}Warning: Could not determine fzf version, skipping${NC}\n"
    return 1
  fi

  # Strip 'v' prefix from version for URL construction
  fzf_version="${fzf_version#v}"

  fzf_url=""

  # Determine fzf download URL based on platform
  if [ "$OS" = "darwin" ]; then
    if [ "$ARCH" = "aarch64" ]; then
      fzf_url="https://github.com/junegunn/fzf/releases/download/v${fzf_version}/fzf-${fzf_version}-darwin_arm64.tar.gz"
    else
      fzf_url="https://github.com/junegunn/fzf/releases/download/v${fzf_version}/fzf-${fzf_version}-darwin_amd64.tar.gz"
    fi
  elif [ "$OS" = "linux" ]; then
    if is_android; then
      # For Android, use the Linux arm64 binary
      fzf_url="https://github.com/junegunn/fzf/releases/download/v${fzf_version}/fzf-${fzf_version}-android_arm64.tar.gz"
    elif [ "$ARCH" = "aarch64" ]; then
      fzf_url="https://github.com/junegunn/fzf/releases/download/v${fzf_version}/fzf-${fzf_version}-linux_arm64.tar.gz"
    else
      fzf_url="https://github.com/junegunn/fzf/releases/download/v${fzf_version}/fzf-${fzf_version}-linux_amd64.tar.gz"
    fi
  elif echo "$OS" | grep -qE 'msys|mingw|cygwin|windows'; then
    fzf_url="https://github.com/junegunn/fzf/releases/download/v${fzf_version}/fzf-${fzf_version}-windows_amd64.zip"
  else
    printf "${YELLOW}Warning: fzf not supported on %s, skipping${NC}\n" "$OS"
    return 1
  fi

  fzf_temp="$TMP_DIR/fzf-${fzf_version}"
  mkdir -p "$fzf_temp"

  if download_file "$fzf_url" "$fzf_temp/fzf_archive"; then
    # Extract based on archive type
    if echo "$fzf_url" | grep -q '\.zip$'; then
      if command -v unzip > /dev/null 2>&1; then
        unzip -q "$fzf_temp/fzf_archive" -d "$fzf_temp"
      else
        printf "${YELLOW}Warning: unzip not found, cannot extract fzf${NC}\n"
        return 1
      fi
    else
      tar -xzf "$fzf_temp/fzf_archive" -C "$fzf_temp"
    fi

    # Find and install the binary
    if [ -f "$fzf_temp/$fzf_binary" ]; then
      cp "$fzf_temp/$fzf_binary" "$managed_fzf_path"
    elif [ -f "$fzf_temp/fzf" ]; then
      cp "$fzf_temp/fzf" "$managed_fzf_path"
    else
      printf "${YELLOW}Warning: Could not find fzf binary in archive${NC}\n"
      return 1
    fi

    chmod +x "$managed_fzf_path"
    prepend_to_path "$INSTALL_DIR"

    # Verify the freshly installed binary meets the minimum version requirement.
    installed_fzf_version=$(get_fzf_version "$managed_fzf_path")
    if [ -z "$installed_fzf_version" ]; then
      printf "${YELLOW}Warning: Installed fzf binary could not be executed${NC}\n"
      return 1
    fi
    if version_less_than "$installed_fzf_version" "0.48.0"; then
      printf "${YELLOW}Warning: Downloaded fzf %s is still < 0.48.0${NC}\n" "$installed_fzf_version"
      return 1
    fi

    printf "${GREEN}✓ fzf %s installed successfully${NC}\n" "$installed_fzf_version"
  else
    printf "${YELLOW}Warning: Failed to download fzf, skipping${NC}\n"
    return 1
  fi

  rm -rf "$fzf_temp"
  return 0
}

# Function to install bat
install_bat() {
  if check_tool_installed "bat"; then
    return 0
  fi

  printf "${BLUE}Installing bat...${NC}\n"

  bat_version=$(get_latest_version "sharkdp/bat")
  if [ -z "$bat_version" ]; then
    printf "${YELLOW}Warning: Could not determine bat version, skipping${NC}\n"
    return 1
  fi

  # Strip 'v' prefix from version for URL construction
  bat_version="${bat_version#v}"

  bat_url=""
  bat_binary="bat"

  # Determine bat download URL based on platform
  if [ "$OS" = "darwin" ]; then
    if [ "$ARCH" = "aarch64" ]; then
      bat_url="https://github.com/sharkdp/bat/releases/download/v${bat_version}/bat-v${bat_version}-aarch64-apple-darwin.tar.gz"
    else
      bat_url="https://github.com/sharkdp/bat/releases/download/v${bat_version}/bat-v${bat_version}-x86_64-apple-darwin.tar.gz"
    fi
  elif [ "$OS" = "linux" ]; then
    if is_android; then
      # For Android, use the Linux musl arm64 build
      bat_url="https://github.com/sharkdp/bat/releases/download/v${bat_version}/bat-v${bat_version}-aarch64-unknown-linux-musl.tar.gz"
    elif [ "$ARCH" = "aarch64" ]; then
      bat_url="https://github.com/sharkdp/bat/releases/download/v${bat_version}/bat-v${bat_version}-aarch64-unknown-linux-musl.tar.gz"
    else
      bat_url="https://github.com/sharkdp/bat/releases/download/v${bat_version}/bat-v${bat_version}-x86_64-unknown-linux-musl.tar.gz"
    fi
  elif echo "$OS" | grep -qE 'msys|mingw|cygwin|windows'; then
    bat_url="https://github.com/sharkdp/bat/releases/download/v${bat_version}/bat-v${bat_version}-x86_64-pc-windows-msvc.zip"
    bat_binary="bat.exe"
  else
    printf "${YELLOW}Warning: bat not supported on %s, skipping${NC}\n" "$OS"
    return 1
  fi

  bat_temp="$TMP_DIR/bat-${bat_version}"
  mkdir -p "$bat_temp"

  if download_file "$bat_url" "$bat_temp/bat_archive"; then
    # Extract based on archive type
    if echo "$bat_url" | grep -q '\.zip$'; then
      if command -v unzip > /dev/null 2>&1; then
        unzip -q "$bat_temp/bat_archive" -d "$bat_temp"
      else
        printf "${YELLOW}Warning: unzip not found, cannot extract bat${NC}\n"
        return 1
      fi
    else
      tar -xzf "$bat_temp/bat_archive" -C "$bat_temp"
    fi

    # Find and install the binary
    bat_extracted_dir=$(find "$bat_temp" -mindepth 1 -maxdepth 1 -type d -name "bat-*" | head -n 1)
    if [ -n "$bat_extracted_dir" ] && [ -f "$bat_extracted_dir/$bat_binary" ]; then
      cp "$bat_extracted_dir/$bat_binary" "$INSTALL_DIR/$bat_binary"
      chmod +x "$INSTALL_DIR/$bat_binary"
      printf "${GREEN}✓ bat installed successfully${NC}\n"
    elif [ -f "$bat_temp/$bat_binary" ]; then
      cp "$bat_temp/$bat_binary" "$INSTALL_DIR/$bat_binary"
      chmod +x "$INSTALL_DIR/$bat_binary"
      printf "${GREEN}✓ bat installed successfully${NC}\n"
    else
      printf "${YELLOW}Warning: Could not find bat binary in archive${NC}\n"
      return 1
    fi
  else
    printf "${YELLOW}Warning: Failed to download bat, skipping${NC}\n"
    return 1
  fi

  rm -rf "$bat_temp"
  return 0
}

# Function to install fd
install_fd() {
  if check_tool_installed "fd"; then
    return 0
  fi

  printf "${BLUE}Installing fd...${NC}\n"

  fd_version=$(get_latest_version "sharkdp/fd")
  if [ -z "$fd_version" ]; then
    printf "${YELLOW}Warning: Could not determine fd version, skipping${NC}\n"
    return 1
  fi

  # Strip 'v' prefix from version for URL construction
  fd_version="${fd_version#v}"

  fd_url=""
  fd_binary="fd"

  # Determine fd download URL based on platform
  if [ "$OS" = "darwin" ]; then
    if [ "$ARCH" = "aarch64" ]; then
      fd_url="https://github.com/sharkdp/fd/releases/download/v${fd_version}/fd-v${fd_version}-aarch64-apple-darwin.tar.gz"
    else
      fd_url="https://github.com/sharkdp/fd/releases/download/v${fd_version}/fd-v${fd_version}-x86_64-apple-darwin.tar.gz"
    fi
  elif [ "$OS" = "linux" ]; then
    if is_android; then
      # For Android, use the Linux musl arm64 build
      fd_url="https://github.com/sharkdp/fd/releases/download/v${fd_version}/fd-v${fd_version}-aarch64-unknown-linux-musl.tar.gz"
    elif [ "$ARCH" = "aarch64" ]; then
      fd_url="https://github.com/sharkdp/fd/releases/download/v${fd_version}/fd-v${fd_version}-aarch64-unknown-linux-musl.tar.gz"
    else
      fd_url="https://github.com/sharkdp/fd/releases/download/v${fd_version}/fd-v${fd_version}-x86_64-unknown-linux-musl.tar.gz"
    fi
  elif echo "$OS" | grep -qE 'msys|mingw|cygwin|windows'; then
    fd_url="https://github.com/sharkdp/fd/releases/download/v${fd_version}/fd-v${fd_version}-x86_64-pc-windows-msvc.zip"
    fd_binary="fd.exe"
  else
    printf "${YELLOW}Warning: fd not supported on %s, skipping${NC}\n" "$OS"
    return 1
  fi

  fd_temp="$TMP_DIR/fd-${fd_version}"
  mkdir -p "$fd_temp"

  if download_file "$fd_url" "$fd_temp/fd_archive"; then
    # Extract based on archive type
    if echo "$fd_url" | grep -q '\.zip$'; then
      if command -v unzip > /dev/null 2>&1; then
        unzip -q "$fd_temp/fd_archive" -d "$fd_temp"
      else
        printf "${YELLOW}Warning: unzip not found, cannot extract fd${NC}\n"
        return 1
      fi
    else
      tar -xzf "$fd_temp/fd_archive" -C "$fd_temp"
    fi

    # Find and install the binary
    fd_extracted_dir=$(find "$fd_temp" -mindepth 1 -maxdepth 1 -type d -name "fd-*" | head -n 1)
    if [ -n "$fd_extracted_dir" ] && [ -f "$fd_extracted_dir/$fd_binary" ]; then
      cp "$fd_extracted_dir/$fd_binary" "$INSTALL_DIR/$fd_binary"
      chmod +x "$INSTALL_DIR/$fd_binary"
      printf "${GREEN}✓ fd installed successfully${NC}\n"
    elif [ -f "$fd_temp/$fd_binary" ]; then
      cp "$fd_temp/$fd_binary" "$INSTALL_DIR/$fd_binary"
      chmod +x "$INSTALL_DIR/$fd_binary"
      printf "${GREEN}✓ fd installed successfully${NC}\n"
    else
      printf "${YELLOW}Warning: Could not find fd binary in archive${NC}\n"
      return 1
    fi
  else
    printf "${YELLOW}Warning: Failed to download fd, skipping${NC}\n"
    return 1
  fi

  rm -rf "$fd_temp"
  return 0
}

# Remove any previously npm-installed forge by trying every known uninstall
# path. Failures are silently ignored — the native binary install that follows
# will overwrite whatever remains.
handle_existing_installation() {
  printf "${BLUE}Removing any previous npm-managed forge installation...${NC}\n"

  # volta — has its own package registry separate from npm
  volta uninstall forgecode 2> /dev/null || true

  # plain npm (covers nvm, fnm, n, system npm, and most other managers
  # whose packages are visible to "npm uninstall -g")
  npm uninstall -g forgecode 2> /dev/null || true

  # Trigger shim regeneration for managers that maintain their own shim dirs,
  # so stale forge shims are cleaned up before the new binary lands.
  asdf reshim nodejs 2> /dev/null || true
  mise reshim 2> /dev/null || true
  nodenv rehash 2> /dev/null || true
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
    printf "${RED}Unsupported architecture: %s${NC}\n" "$ARCH"
    printf "${YELLOW}Supported architectures: x86_64, aarch64${NC}\n"
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
  if command -v getprop > /dev/null 2>&1; then
    if getprop ro.build.version.release > /dev/null 2>&1; then
      return 0
    fi
  fi

  return 1
}

# Get libc type and glibc compatibility
get_libc_info() {
  # Check for musl library files first (faster and more reliable)
  if [ -f "/lib/libc.musl-x86_64.so.1" ] || [ -f "/lib/libc.musl-aarch64.so.1" ]; then
    echo "musl"
    return
  fi

  # Find ls binary dynamically (more portable)
  libc_ls_binary=$(command -v ls 2> /dev/null || echo "/bin/ls")

  # Check if ldd reports musl (if ldd exists)
  if command -v ldd > /dev/null 2>&1; then
    if ldd "$libc_ls_binary" 2>&1 | grep -q musl; then
      echo "musl"
      return
    fi
  fi

  # Try ldd for glibc version (if ldd exists)
  if command -v ldd > /dev/null 2>&1; then
    libc_ldd_output=$(ldd --version 2>&1 | head -n 1 || true)

    # Double-check it's not musl
    if echo "$libc_ldd_output" | grep -qiF "musl"; then
      echo "musl"
      return
    fi

    # Extract glibc version
    libc_version=$(echo "$libc_ldd_output" | grep -oE '[0-9]+\.[0-9]+' | head -n 1)

    # If no version found from ldd, try getconf
    if [ -z "$libc_version" ]; then
      if command -v getconf > /dev/null 2>&1; then
        libc_getconf_output=$(getconf GNU_LIBC_VERSION 2> /dev/null || true)
        libc_version=$(echo "$libc_getconf_output" | grep -oE '[0-9]+\.[0-9]+' | head -n 1)
      fi
    fi

    # If we have a version, check if it's sufficient (>= 2.39)
    if [ -n "$libc_version" ]; then
      # Convert version to comparable number (e.g., 2.39 -> 239)
      libc_major=$(echo "$libc_version" | cut -d. -f1)
      libc_minor=$(echo "$libc_version" | cut -d. -f2)
      libc_version_num=$((libc_major * 100 + libc_minor))

      # Our binary requires glibc 2.39 or higher
      if [ "$libc_version_num" -ge 239 ]; then
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
  TARGET_EXT=""
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
      TARGET_EXT=""
      # Prefer user-local directory to avoid sudo
      INSTALL_DIR="$HOME/.local/bin"
      USE_SUDO=false
      ;;
    darwin)
      TARGET="$ARCH-apple-darwin"
      BINARY_NAME="forge"
      TARGET_EXT=""
      # Prefer user-local directory to avoid sudo
      INSTALL_DIR="$HOME/.local/bin"
      USE_SUDO=false
      ;;
    msys* | mingw* | cygwin* | windows*)
      TARGET="$ARCH-pc-windows-msvc"
      BINARY_NAME="forge.exe"
      TARGET_EXT=".exe"
      # Windows install to user's local bin or AppData
      if [ -n "$LOCALAPPDATA" ]; then
        INSTALL_DIR="$LOCALAPPDATA/Programs/Forge"
      else
        INSTALL_DIR="$HOME/.local/bin"
      fi
      USE_SUDO=false
      ;;
    *)
      printf "${RED}Unsupported operating system: %s${NC}\n" "$OS"
      printf "${YELLOW}Supported operating systems: Linux, macOS (Darwin), Windows${NC}\n"
      printf "${BLUE}For installation instructions, visit:${NC}\n"
      printf "${BLUE}https://github.com/Zetkolink/forgecode#installation${NC}\n"
      exit 1
      ;;
  esac
fi

printf "${BLUE}Detected platform: %s${NC}\n" "$TARGET"

# Check for an existing installation and clean up npm-managed versions
handle_existing_installation

# Allow optional version argument, defaulting to "latest"
VERSION="${1:-latest}"

# Construct download URLs
if [ "$VERSION" = "latest" ]; then
  DOWNLOAD_URLS="https://github.com/Zetkolink/forgecode/releases/latest/download/forge-$TARGET$TARGET_EXT"
else
  DOWNLOAD_URLS="https://github.com/Zetkolink/forgecode/releases/download/$VERSION/forge-$TARGET$TARGET_EXT"
  case "$VERSION" in
    v*) ;;

    *)
      DOWNLOAD_URLS="$DOWNLOAD_URLS https://github.com/Zetkolink/forgecode/releases/download/v$VERSION/forge-$TARGET$TARGET_EXT"
      ;;
  esac
fi

# Create temp directory
TMP_DIR=$(mktemp -d)
TEMP_BINARY="$TMP_DIR/$BINARY_NAME"

# Download Forge
download_success=false
for DOWNLOAD_URL in $DOWNLOAD_URLS; do
  printf "${BLUE}Downloading Forge from %s...${NC}\n" "$DOWNLOAD_URL"
  if download_file "$DOWNLOAD_URL" "$TEMP_BINARY"; then
    download_success=true
    break
  fi
done

if [ "$download_success" != "true" ]; then
  printf "${RED}Failed to download Forge.${NC}\n" >&2
  printf "${YELLOW}Please check:${NC}\n" >&2
  printf "  - Your internet connection\n" >&2
  printf "  - The version '%s' exists\n" "$VERSION" >&2
  printf "  - The target '%s' is supported\n" "$TARGET" >&2
  rm -rf "$TMP_DIR"
  exit 1
fi

# Create install directory if it doesn't exist
if [ ! -d "$INSTALL_DIR" ]; then
  printf "${BLUE}Creating installation directory: %s${NC}\n" "$INSTALL_DIR"
  if [ "$USE_SUDO" = true ]; then
    sudo mkdir -p "$INSTALL_DIR"
  else
    mkdir -p "$INSTALL_DIR"
  fi
fi

# Install
INSTALL_PATH="$INSTALL_DIR/$BINARY_NAME"
printf "${BLUE}Installing to %s...${NC}\n" "$INSTALL_PATH"
if [ "$USE_SUDO" = true ]; then
  sudo mv "$TEMP_BINARY" "$INSTALL_PATH"
  sudo chmod +x "$INSTALL_PATH"
else
  mv "$TEMP_BINARY" "$INSTALL_PATH"
  chmod +x "$INSTALL_PATH"
fi

# Ensure future shells prefer the user-local install directory.
ensure_install_dir_shell_path

# Add to PATH if necessary (for Windows or non-standard install locations)
if [ "$OS" = "windows" ] || [ "$OS" = "msys" ] || [ "$OS" = "mingw" ] || [ "$OS" = "cygwin" ]; then
  if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
    printf "${YELLOW}Note: You may need to add %s to your PATH${NC}\n" "$INSTALL_DIR"
  fi
fi

# Verify installation
printf "\n"
if command -v forge > /dev/null 2>&1; then
  printf "${GREEN}✓ Forge has been successfully installed!${NC}\n"
  forge --version 2> /dev/null || true
  printf "${BLUE}Run 'forge' to get started.${NC}\n"
else
  printf "${GREEN}✓ Forge has been installed to %s${NC}\n" "$INSTALL_PATH"
  printf "\n"
  printf "${YELLOW}The 'forge' command is not in your PATH yet.${NC}\n"

  # Check if the install directory is in PATH
  if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
    printf "${BLUE}Add it to your PATH by running:${NC}\n"

    # Detect shell from SHELL variable and provide appropriate instructions
    shell_name=$(basename "${SHELL:-sh}")
    case "$shell_name" in
      zsh)
        printf "  echo 'export PATH=\"%s:\$PATH\"' >> ~/.zshrc\n" "$INSTALL_DIR"
        printf "  source ~/.zshrc\n"
        ;;
      bash)
        printf "  echo 'export PATH=\"%s:\$PATH\"' >> ~/.bashrc\n" "$INSTALL_DIR"
        printf "  source ~/.bashrc\n"
        ;;
      fish)
        printf "  fish_add_path %s\n" "$INSTALL_DIR"
        ;;
      *)
        printf "  export PATH=\"%s:\$PATH\"\n" "$INSTALL_DIR"
        ;;
    esac
  else
    printf "${BLUE}Restart your shell or run:${NC}\n"

    # Detect shell and provide appropriate source command
    shell_name=$(basename "${SHELL:-sh}")
    case "$shell_name" in
      zsh)
        printf "  source ~/.zshrc\n"
        ;;
      bash)
        printf "  source ~/.bashrc\n"
        ;;
      fish)
        printf "  Restart your terminal (fish doesn't need source)\n"
        ;;
      *)
        printf "  Restart your terminal\n"
        ;;
    esac
  fi
fi

# Install dependencies (fzf, bat, fd)
printf "\n"
printf "${BLUE}Installing dependencies...${NC}\n"
install_fzf || true
install_bat || true
install_fd || true

printf "\n"
printf "${GREEN}Installation complete!${NC}\n"
printf "${BLUE}Tools installed: forge, fzf, bat, fd${NC}\n"
printf "${YELLOW}Because this installer runs via '| sh', your current shell may still use old PATH values.${NC}\n"
shell_name=$(basename "${SHELL:-sh}")
case "$shell_name" in
  zsh)
    printf "${BLUE}Open a new terminal or run: exec zsh${NC}\n"
    ;;
  bash)
    printf "${BLUE}Open a new terminal or run: exec bash${NC}\n"
    ;;
  fish)
    printf "${BLUE}Open a new terminal or run: exec fish${NC}\n"
    ;;
  *)
    printf "${BLUE}Open a new terminal to pick up the updated PATH.${NC}\n"
    ;;
esac

# Cleanup temp directory
rm -rf "$TMP_DIR"
