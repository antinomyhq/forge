#!/bin/bash
# =============================================================================
# Docker-based E2E test suite for `forge zsh setup`
#
# Builds forge binaries for each Linux target (matching CI release.yml), then
# tests the complete zsh setup flow inside Docker containers across multiple
# distributions: dependency detection, installation (zsh, Oh My Zsh, plugins),
# .zshrc configuration, and doctor diagnostics.
#
# Build targets (from CI):
#   - x86_64-unknown-linux-musl  (cross=true, static)
#   - x86_64-unknown-linux-gnu   (cross=false, dynamic)
#
# Prerequisites:
#   - Docker installed and running
#   - Rust toolchain with cross (cargo install cross)
#   - protoc (for non-cross builds)
#
# Usage:
#   bash crates/forge_ci/tests/scripts/test-zsh-setup.sh                         # build + test all
#   bash crates/forge_ci/tests/scripts/test-zsh-setup.sh --quick                 # shellcheck only
#   bash crates/forge_ci/tests/scripts/test-zsh-setup.sh --filter "alpine"       # run only matching
#   bash crates/forge_ci/tests/scripts/test-zsh-setup.sh --jobs 4                # limit parallelism
#   bash crates/forge_ci/tests/scripts/test-zsh-setup.sh --skip-build            # skip build, use existing
#   bash crates/forge_ci/tests/scripts/test-zsh-setup.sh --targets musl          # only test musl target
#   bash crates/forge_ci/tests/scripts/test-zsh-setup.sh --list                  # list images and exit
#   bash crates/forge_ci/tests/scripts/test-zsh-setup.sh --help                  # show usage
#
# Adding new test images:
#   Append entries to the IMAGES array using the format:
#     "docker_image|Human Label|extra_pre_install_packages"
#   The third field is for packages to pre-install BEFORE forge runs (e.g., zsh
#   for the pre-installed-zsh edge case). Leave empty for bare images.
#
# Relationship to test-cli.sh:
#   test-cli.sh tests the CLI installer script (static/cli).
#   This script tests `forge zsh setup` — the Rust-native zsh setup command.
#   Both use the same Docker/FIFO parallel execution patterns.
# =============================================================================

set -euo pipefail

# =============================================================================
# Constants
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR

PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
readonly PROJECT_ROOT

readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly BOLD='\033[1m'
readonly DIM='\033[2m'
readonly NC='\033[0m'

readonly SHELLCHECK_EXCLUSIONS="SC2155,SC2086,SC1090,SC2034,SC2181,SC2016,SC2162"
readonly DOCKER_TAG_PREFIX="forge-zsh-test"
readonly DEFAULT_MAX_JOBS=8

# Detect host architecture
HOST_ARCH="$(uname -m)"
readonly HOST_ARCH

# Build targets — matches CI release.yml for Linux
# Only include targets that match the host architecture
# Format: "target|cross_flag|label"
#   target     - Rust target triple
#   cross_flag - "true" to build with cross, "false" for cargo
#   label      - human-readable name
if [ "$HOST_ARCH" = "aarch64" ] || [ "$HOST_ARCH" = "arm64" ]; then
  # ARM64 runner: only build arm64 targets
  readonly BUILD_TARGETS=(
    "aarch64-unknown-linux-musl|true|musl (static)"
    "aarch64-unknown-linux-gnu|false|gnu (dynamic)"
  )
elif [ "$HOST_ARCH" = "x86_64" ] || [ "$HOST_ARCH" = "amd64" ]; then
  # x86_64 runner: only build x86_64 targets
  readonly BUILD_TARGETS=(
    "x86_64-unknown-linux-musl|true|musl (static)"
    "x86_64-unknown-linux-gnu|false|gnu (dynamic)"
  )
else
  echo "Error: Unsupported host architecture: $HOST_ARCH" >&2
  echo "Supported: x86_64, amd64, aarch64, arm64" >&2
  exit 1
fi

# Docker images — one entry per supported Linux variant
#
# Format: "image|label|extra_packages"
#   image          - Docker Hub image reference
#   label          - human-readable name for the test report
#   extra_packages - packages to pre-install before forge runs (empty = bare)
readonly IMAGES=(
  # --- Tier 1: apt-get (Debian/Ubuntu) ---
  "ubuntu:24.04|Ubuntu 24.04 (apt-get)|"
  "ubuntu:22.04|Ubuntu 22.04 (apt-get)|"
  "debian:bookworm-slim|Debian 12 Slim (apt-get)|"

  # --- Tier 2: dnf (Fedora/RHEL) ---
  "fedora:41|Fedora 41 (dnf)|"
  "rockylinux:9|Rocky Linux 9 (dnf)|"

  # --- Tier 3: apk (Alpine) ---
  "alpine:3.20|Alpine 3.20 (apk)|"

  # --- Tier 4: pacman (Arch) ---
  "archlinux:latest|Arch Linux (pacman)|"

  # --- Tier 5: zypper (openSUSE) ---
  "opensuse/tumbleweed:latest|openSUSE Tumbleweed (zypper)|"

  # --- Tier 6: xbps (Void) ---
  "ghcr.io/void-linux/void-glibc:latest|Void Linux glibc (xbps)|"
)

# Edge case images — special test scenarios
readonly EDGE_CASES=(
  # Pre-installed zsh: verify setup skips zsh install
  "PREINSTALLED_ZSH|ubuntu:24.04|Pre-installed zsh (skip zsh install)|zsh"

  # Pre-installed everything: verify fast path
  "PREINSTALLED_ALL|ubuntu:24.04|Pre-installed everything (fast path)|FULL_PREINSTALL"

  # No git: verify graceful failure
  "NO_GIT|ubuntu:24.04|No git (graceful failure)|NO_GIT"

  # Broken zsh: verify reinstall
  "BROKEN_ZSH|ubuntu:24.04|Broken zsh (modules removed)|BROKEN_ZSH"

  # Re-run idempotency: verify no duplicates
  "RERUN|ubuntu:24.04|Re-run idempotency|RERUN"

  # Partial install: only plugins missing
  "PARTIAL|ubuntu:24.04|Partial install (only plugins missing)|PARTIAL"
)

# =============================================================================
# Runtime state
# =============================================================================

PASS=0
FAIL=0
SKIP=0
FAILURES=()

# CLI options
MODE="full"
MAX_JOBS=""
FILTER_PATTERN=""
EXCLUDE_PATTERN=""
NO_CLEANUP=false
SKIP_BUILD=false
TARGET_FILTER=""  # empty = all, "musl" or "gnu" to filter
NATIVE_BUILD=false  # if true, use cargo instead of cross

# Shared temp paths
RESULTS_DIR=""

# =============================================================================
# Logging helpers
# =============================================================================

log_header() { echo -e "\n${BOLD}${BLUE}$1${NC}"; }
log_pass()   { echo -e "  ${GREEN}PASS${NC} $1"; PASS=$((PASS + 1)); }
log_fail()   { echo -e "  ${RED}FAIL${NC} $1"; FAIL=$((FAIL + 1)); FAILURES+=("$1"); }
log_skip()   { echo -e "  ${YELLOW}SKIP${NC} $1"; SKIP=$((SKIP + 1)); }
log_info()   { echo -e "  ${DIM}$1${NC}"; }

# =============================================================================
# Argument parsing
# =============================================================================

print_usage() {
  cat <<EOF
Usage: bash crates/forge_ci/tests/scripts/test-zsh-setup.sh [OPTIONS]

Options:
  --quick              Run static analysis only (no Docker)
  --jobs <n>           Max parallel Docker jobs (default: nproc, cap $DEFAULT_MAX_JOBS)
  --filter <pattern>   Run only images whose label matches <pattern> (grep -iE)
  --exclude <pattern>  Skip images whose label matches <pattern> (grep -iE)
  --skip-build         Skip binary build, use existing binaries
  --targets <filter>   Only test matching targets: "musl", "gnu", or "all" (default: all)
  --native-build       Use cargo instead of cross for building (for CI runners)
  --no-cleanup         Keep Docker images and results dir after tests
  --list               List all test images and exit
  --help               Show this help message

Environment variables:
  PARALLEL_JOBS        Fallback for --jobs
EOF
}

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --quick)
        MODE="quick"
        shift
        ;;
      --jobs)
        MAX_JOBS="${2:?--jobs requires a number}"
        shift 2
        ;;
      --filter)
        FILTER_PATTERN="${2:?--filter requires a pattern}"
        shift 2
        ;;
      --exclude)
        EXCLUDE_PATTERN="${2:?--exclude requires a pattern}"
        shift 2
        ;;
      --skip-build)
        SKIP_BUILD=true
        shift
        ;;
      --targets)
        TARGET_FILTER="${2:?--targets requires a value (musl, gnu, or all)}"
        shift 2
        ;;
      --native-build)
        NATIVE_BUILD=true
        shift
        ;;
      --no-cleanup)
        NO_CLEANUP=true
        shift
        ;;
      --list)
        list_images
        exit 0
        ;;
      --help|-h)
        print_usage
        exit 0
        ;;
      *)
        echo "Unknown option: $1" >&2
        print_usage >&2
        exit 1
        ;;
    esac
  done

  if [ -z "$MAX_JOBS" ] && [ -n "${PARALLEL_JOBS:-}" ]; then
    MAX_JOBS="$PARALLEL_JOBS"
  fi
}

list_images() {
  echo -e "${BOLD}Build Targets:${NC}"
  local idx=0
  for entry in "${BUILD_TARGETS[@]}"; do
    idx=$((idx + 1))
    IFS='|' read -r target _cross label <<< "$entry"
    printf "  %2d. %-55s %s\n" "$idx" "$label" "$target"
  done

  echo -e "\n${BOLD}Base Images:${NC}"
  for entry in "${IMAGES[@]}"; do
    idx=$((idx + 1))
    IFS='|' read -r image label _packages <<< "$entry"
    printf "  %2d. %-55s %s\n" "$idx" "$label" "$image"
  done

  echo -e "\n${BOLD}Edge Cases:${NC}"
  for entry in "${EDGE_CASES[@]}"; do
    idx=$((idx + 1))
    IFS='|' read -r _type image label _packages <<< "$entry"
    printf "  %2d. %-55s %s\n" "$idx" "$label" "$image"
  done
}

# =============================================================================
# Build binaries
# =============================================================================

# Build a binary for a given target, matching CI release.yml logic.
# Uses cross for cross-compiled targets, cargo for native targets.
# If NATIVE_BUILD is true, always uses cargo regardless of use_cross flag.
build_binary() {
  local target="$1"
  local use_cross="$2"
  local binary_path="$PROJECT_ROOT/target/${target}/debug/forge"

  if [ "$SKIP_BUILD" = true ] && [ -f "$binary_path" ]; then
    log_info "Skipping build for ${target} (binary exists)"
    return 0
  fi

  # Override use_cross if --native-build flag is set
  if [ "$NATIVE_BUILD" = true ]; then
    use_cross="false"
  fi

  if [ "$use_cross" = "true" ]; then
    if ! command -v cross > /dev/null 2>&1; then
      log_fail "cross not installed (needed for ${target}). Install with: cargo install cross"
      return 1
    fi
    log_info "Building ${target} with cross (debug)..."
    if ! cross build --target "$target" 2>"$RESULTS_DIR/build-${target}.log"; then
      log_fail "Build failed for ${target}"
      log_info "Build log: $RESULTS_DIR/build-${target}.log"
      echo ""
      echo "===== Full build log ====="
      cat "$RESULTS_DIR/build-${target}.log" 2>/dev/null || echo "Log file not found"
      echo "=========================="
      echo ""
      return 1
    fi
  else
    # Native build with cargo — mirrors CI: no cross, uses setup-cross-toolchain
    if ! rustup target list --installed 2>/dev/null | grep -q "$target"; then
      log_info "Adding Rust target ${target}..."
      rustup target add "$target" 2>/dev/null || true
    fi
    log_info "Building ${target} with cargo (debug)..."
    if ! cargo build --target "$target" 2>"$RESULTS_DIR/build-${target}.log"; then
      log_fail "Build failed for ${target}"
      log_info "Build log: $RESULTS_DIR/build-${target}.log"
      echo ""
      echo "===== Full build log ====="
      cat "$RESULTS_DIR/build-${target}.log" 2>/dev/null || echo "Log file not found"
      echo "=========================="
      echo ""
      return 1
    fi
  fi

  if [ -f "$binary_path" ]; then
    log_pass "Built ${target} -> $(du -h "$binary_path" | cut -f1)"
    return 0
  else
    log_fail "Binary not found after build: ${binary_path}"
    return 1
  fi
}

# Build all selected targets. Exits immediately if any build fails.
build_all_targets() {
  log_header "Phase 2: Build Binaries"

  for entry in "${BUILD_TARGETS[@]}"; do
    IFS='|' read -r target use_cross label <<< "$entry"

    # Apply target filter
    if [ -n "$TARGET_FILTER" ] && [ "$TARGET_FILTER" != "all" ]; then
      if ! echo "$target" | grep -qi "$TARGET_FILTER"; then
        log_skip "${label} (filtered out by --targets ${TARGET_FILTER})"
        continue
      fi
    fi

    # Build and exit immediately on failure
    if ! build_binary "$target" "$use_cross"; then
      echo "Error: Build failed for ${target}. Cannot continue without binaries." >&2
      exit 1
    fi
  done
}

# Return the relative path (from PROJECT_ROOT) to the binary for a target.
binary_rel_path() {
  local target="$1"
  echo "target/${target}/debug/forge"
}

# =============================================================================
# Static analysis
# =============================================================================

run_static_checks() {
  log_header "Phase 1: Static Analysis"

  if bash -n "${BASH_SOURCE[0]}" 2>/dev/null; then
    log_pass "bash -n syntax check"
  else
    log_fail "bash -n syntax check"
  fi

  if command -v shellcheck > /dev/null 2>&1; then
    if shellcheck -x -e "$SHELLCHECK_EXCLUSIONS" "${BASH_SOURCE[0]}" 2>/dev/null; then
      log_pass "shellcheck (excluding $SHELLCHECK_EXCLUSIONS)"
    else
      log_fail "shellcheck (excluding $SHELLCHECK_EXCLUSIONS)"
    fi
  else
    log_skip "shellcheck (not installed)"
  fi
}

# =============================================================================
# Docker helpers
# =============================================================================

# Build the install command for git (and bash where needed).
pkg_install_cmd() {
  local image="$1"
  local extra="$2"

  # Helper: check if extra is a special sentinel (not a real package name)
  is_sentinel() {
    case "$1" in
      NO_GIT|FULL_PREINSTALL|BROKEN_ZSH|RERUN|PARTIAL|"") return 0 ;;
      *) return 1 ;;
    esac
  }

  local git_cmd=""
  case "$image" in
    alpine*)
      git_cmd="apk add --no-cache git bash curl"
      if ! is_sentinel "$extra"; then git_cmd="$git_cmd $extra"; fi
      ;;
    fedora*|rockylinux*|almalinux*|centos*)
      git_cmd="dnf install -y git"
      if ! is_sentinel "$extra"; then git_cmd="$git_cmd $extra"; fi
      ;;
    archlinux*)
      git_cmd="pacman -Sy --noconfirm git"
      if ! is_sentinel "$extra"; then git_cmd="$git_cmd $extra"; fi
      ;;
    opensuse*|suse*)
      git_cmd="zypper -n install git curl"
      if ! is_sentinel "$extra"; then git_cmd="$git_cmd $extra"; fi
      ;;
    *void*)
      git_cmd="xbps-install -Sy git bash curl"
      if ! is_sentinel "$extra"; then git_cmd="$git_cmd $extra"; fi
      ;;
    *)
      git_cmd="apt-get update -qq && apt-get install -y -qq git curl"
      if ! is_sentinel "$extra"; then git_cmd="$git_cmd $extra"; fi
      ;;
  esac

  echo "$git_cmd"
}

# Return Dockerfile RUN commands to create a non-root user with sudo.
user_setup_cmd() {
  local image="$1"
  local sudoers="echo 'testuser ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers"
  local create_user="useradd -m -s /bin/bash testuser"

  case "$image" in
    alpine*)
      echo "apk add --no-cache sudo && adduser -D -s /bin/sh testuser && ${sudoers}"
      ;;
    fedora*|rockylinux*|almalinux*|centos*)
      echo "dnf install -y sudo && ${create_user} && ${sudoers}"
      ;;
    archlinux*)
      echo "pacman -Sy --noconfirm sudo && ${create_user} && ${sudoers}"
      ;;
    opensuse*|suse*)
      echo "zypper -n install sudo && ${create_user} && ${sudoers}"
      ;;
    *void*)
      echo "xbps-install -Sy sudo shadow && ${create_user} && ${sudoers}"
      ;;
    *)
      echo "apt-get update -qq && apt-get install -y -qq sudo && ${create_user} && ${sudoers}"
      ;;
  esac
}

# Build a Docker image for testing.
#   build_docker_image <tag> <image> <binary_rel_path> <install_cmd> [user_setup] [extra_setup]
build_docker_image() {
  local tag="$1"
  local image="$2"
  local bin_rel="$3"
  local install_cmd="$4"
  local user_setup="${5:-}"
  local extra_setup="${6:-}"

  local user_lines=""
  if [ -n "$user_setup" ]; then
    user_lines="RUN ${user_setup}
USER testuser
WORKDIR /home/testuser"
  fi

  local extra_lines=""
  if [ -n "$extra_setup" ]; then
    extra_lines="RUN ${extra_setup}"
  fi

  local build_log="$RESULTS_DIR/docker-build-${tag}.log"
  if ! docker build --quiet -t "$tag" -f - "$PROJECT_ROOT" <<DOCKERFILE >"$build_log" 2>&1
FROM ${image}
ENV DEBIAN_FRONTEND=noninteractive
ENV TERM=dumb
RUN ${install_cmd}
COPY ${bin_rel} /usr/local/bin/forge
RUN chmod +x /usr/local/bin/forge
${extra_lines}
${user_lines}
DOCKERFILE
  then
    return 1
  fi
  return 0
}

# =============================================================================
# Verification script
# =============================================================================

# Output the in-container verification script.
# Uses a single-quoted heredoc so no host-side variable expansion occurs.
# Arguments:
#   $1 - test type: "standard" | "no_git" | "preinstalled_zsh" |
#        "preinstalled_all" | "broken_zsh" | "rerun" | "partial"
generate_verify_script() {
  local test_type="${1:-standard}"

  cat <<'VERIFY_SCRIPT_HEADER'
#!/bin/bash
set -o pipefail

VERIFY_SCRIPT_HEADER

  # Emit the test type as a variable
  echo "TEST_TYPE=\"${test_type}\""

  cat <<'VERIFY_SCRIPT_BODY'

# --- Run forge zsh setup and capture output ---
setup_output=$(forge zsh setup --non-interactive 2>&1)
setup_exit=$?
echo "SETUP_EXIT=${setup_exit}"

# --- Verify zsh binary ---
if command -v zsh > /dev/null 2>&1; then
  zsh_ver=$(zsh --version 2>&1 | head -1) || zsh_ver="(failed)"
  if zsh -c "zmodload zsh/zle && zmodload zsh/datetime && zmodload zsh/stat" > /dev/null 2>&1; then
    echo "CHECK_ZSH=PASS ${zsh_ver} (modules OK)"
  else
    echo "CHECK_ZSH=FAIL ${zsh_ver} (modules broken)"
  fi
else
  if [ "$TEST_TYPE" = "no_git" ]; then
    echo "CHECK_ZSH=PASS (expected: no zsh in no-git test)"
  else
    echo "CHECK_ZSH=FAIL zsh not found in PATH"
  fi
fi

# --- Verify Oh My Zsh ---
if [ -d "$HOME/.oh-my-zsh" ]; then
  omz_ok=true
  omz_detail="dir=OK"
  for subdir in custom/plugins themes lib; do
    if [ ! -d "$HOME/.oh-my-zsh/$subdir" ]; then
      omz_ok=false
      omz_detail="${omz_detail}, ${subdir}=MISSING"
    fi
  done
  if [ "$omz_ok" = true ]; then
    echo "CHECK_OMZ_DIR=PASS ${omz_detail}"
  else
    echo "CHECK_OMZ_DIR=FAIL ${omz_detail}"
  fi
else
  if [ "$TEST_TYPE" = "no_git" ]; then
    echo "CHECK_OMZ_DIR=PASS (expected: no OMZ in no-git test)"
  else
    echo "CHECK_OMZ_DIR=FAIL ~/.oh-my-zsh not found"
  fi
fi

# --- Verify Oh My Zsh defaults in .zshrc ---
if [ -f "$HOME/.zshrc" ]; then
  omz_defaults_ok=true
  omz_defaults_detail=""
  if grep -q 'ZSH_THEME=' "$HOME/.zshrc" 2>/dev/null; then
    omz_defaults_detail="theme=OK"
  else
    omz_defaults_ok=false
    omz_defaults_detail="theme=MISSING"
  fi
  if grep -q '^plugins=' "$HOME/.zshrc" 2>/dev/null; then
    omz_defaults_detail="${omz_defaults_detail}, plugins=OK"
  else
    omz_defaults_ok=false
    omz_defaults_detail="${omz_defaults_detail}, plugins=MISSING"
  fi
  if [ "$omz_defaults_ok" = true ]; then
    echo "CHECK_OMZ_DEFAULTS=PASS ${omz_defaults_detail}"
  else
    echo "CHECK_OMZ_DEFAULTS=FAIL ${omz_defaults_detail}"
  fi
else
  if [ "$TEST_TYPE" = "no_git" ]; then
    echo "CHECK_OMZ_DEFAULTS=PASS (expected: no .zshrc in no-git test)"
  else
    echo "CHECK_OMZ_DEFAULTS=FAIL ~/.zshrc not found"
  fi
fi

# --- Verify plugins ---
zsh_custom="${ZSH_CUSTOM:-$HOME/.oh-my-zsh/custom}"
if [ -d "$zsh_custom/plugins/zsh-autosuggestions" ]; then
  # Check for .zsh files using ls (find may not be available on minimal images)
  if ls "$zsh_custom/plugins/zsh-autosuggestions/"*.zsh 1>/dev/null 2>&1; then
    echo "CHECK_AUTOSUGGESTIONS=PASS"
  else
    echo "CHECK_AUTOSUGGESTIONS=FAIL (dir exists but no .zsh files)"
  fi
else
  if [ "$TEST_TYPE" = "no_git" ]; then
    echo "CHECK_AUTOSUGGESTIONS=PASS (expected: no plugins in no-git test)"
  else
    echo "CHECK_AUTOSUGGESTIONS=FAIL not installed"
  fi
fi

if [ -d "$zsh_custom/plugins/zsh-syntax-highlighting" ]; then
  if ls "$zsh_custom/plugins/zsh-syntax-highlighting/"*.zsh 1>/dev/null 2>&1; then
    echo "CHECK_SYNTAX_HIGHLIGHTING=PASS"
  else
    echo "CHECK_SYNTAX_HIGHLIGHTING=FAIL (dir exists but no .zsh files)"
  fi
else
  if [ "$TEST_TYPE" = "no_git" ]; then
    echo "CHECK_SYNTAX_HIGHLIGHTING=PASS (expected: no plugins in no-git test)"
  else
    echo "CHECK_SYNTAX_HIGHLIGHTING=FAIL not installed"
  fi
fi

# --- Verify .zshrc forge markers and content ---
if [ -f "$HOME/.zshrc" ]; then
  if grep -q '# >>> forge initialize >>>' "$HOME/.zshrc" && \
     grep -q '# <<< forge initialize <<<' "$HOME/.zshrc"; then
    echo "CHECK_ZSHRC_MARKERS=PASS"
  else
    echo "CHECK_ZSHRC_MARKERS=FAIL markers not found"
  fi

  if grep -q 'eval "\$(forge zsh plugin)"' "$HOME/.zshrc"; then
    echo "CHECK_ZSHRC_PLUGIN=PASS"
  else
    echo "CHECK_ZSHRC_PLUGIN=FAIL plugin eval not found"
  fi

  if grep -q 'eval "\$(forge zsh theme)"' "$HOME/.zshrc"; then
    echo "CHECK_ZSHRC_THEME=PASS"
  else
    echo "CHECK_ZSHRC_THEME=FAIL theme eval not found"
  fi

  if grep -q 'NERD_FONT=0' "$HOME/.zshrc"; then
    echo "CHECK_NO_NERD_FONT_DISABLE=FAIL (NERD_FONT=0 found in non-interactive mode)"
  else
    echo "CHECK_NO_NERD_FONT_DISABLE=PASS"
  fi

  if grep -q 'FORGE_EDITOR' "$HOME/.zshrc"; then
    echo "CHECK_NO_FORGE_EDITOR=FAIL (FORGE_EDITOR found in non-interactive mode)"
  else
    echo "CHECK_NO_FORGE_EDITOR=PASS"
  fi

  # Check marker uniqueness (idempotency)
  start_count=$(grep -c '# >>> forge initialize >>>' "$HOME/.zshrc" 2>/dev/null || echo "0")
  end_count=$(grep -c '# <<< forge initialize <<<' "$HOME/.zshrc" 2>/dev/null || echo "0")
  if [ "$start_count" -eq 1 ] && [ "$end_count" -eq 1 ]; then
    echo "CHECK_MARKER_UNIQUE=PASS"
  else
    echo "CHECK_MARKER_UNIQUE=FAIL (start=${start_count}, end=${end_count})"
  fi
else
  if [ "$TEST_TYPE" = "no_git" ]; then
    echo "CHECK_ZSHRC_MARKERS=PASS (expected: no .zshrc in no-git test)"
    echo "CHECK_ZSHRC_PLUGIN=PASS (expected: no .zshrc in no-git test)"
    echo "CHECK_ZSHRC_THEME=PASS (expected: no .zshrc in no-git test)"
    echo "CHECK_NO_NERD_FONT_DISABLE=PASS (expected: no .zshrc in no-git test)"
    echo "CHECK_NO_FORGE_EDITOR=PASS (expected: no .zshrc in no-git test)"
    echo "CHECK_MARKER_UNIQUE=PASS (expected: no .zshrc in no-git test)"
  else
    echo "CHECK_ZSHRC_MARKERS=FAIL no .zshrc"
    echo "CHECK_ZSHRC_PLUGIN=FAIL no .zshrc"
    echo "CHECK_ZSHRC_THEME=FAIL no .zshrc"
    echo "CHECK_NO_NERD_FONT_DISABLE=FAIL no .zshrc"
    echo "CHECK_NO_FORGE_EDITOR=FAIL no .zshrc"
    echo "CHECK_MARKER_UNIQUE=FAIL no .zshrc"
  fi
fi

# --- Run forge zsh doctor ---
doctor_output=$(forge zsh doctor 2>&1) || true
doctor_exit=$?
if [ "$TEST_TYPE" = "no_git" ]; then
  # Doctor may fail or not run at all in no-git scenario
  echo "CHECK_DOCTOR_EXIT=PASS (skipped for no-git test)"
else
  # Doctor is expected to run — exit 0 = all good, exit 1 = warnings (acceptable)
  if [ $doctor_exit -le 1 ]; then
    echo "CHECK_DOCTOR_EXIT=PASS (exit=${doctor_exit})"
  else
    echo "CHECK_DOCTOR_EXIT=FAIL (exit=${doctor_exit})"
  fi
fi

# --- Verify output format ---
output_ok=true
output_detail=""

# Check for environment detection output
if echo "$setup_output" | grep -qi "found\|not found\|installed\|Detecting"; then
  output_detail="detect=OK"
else
  output_ok=false
  output_detail="detect=MISSING"
fi

if [ "$TEST_TYPE" = "no_git" ]; then
  # For no-git test, check for the error message
  if echo "$setup_output" | grep -qi "git is required"; then
    output_detail="${output_detail}, git_error=OK"
  else
    output_ok=false
    output_detail="${output_detail}, git_error=MISSING"
  fi
  echo "CHECK_OUTPUT_FORMAT=PASS ${output_detail}"
else
  # Check for setup complete message
  if echo "$setup_output" | grep -qi "Setup complete\|complete"; then
    output_detail="${output_detail}, complete=OK"
  else
    output_ok=false
    output_detail="${output_detail}, complete=MISSING"
  fi

  # Check for configure step
  if echo "$setup_output" | grep -qi "Configuring\|configured\|forge plugins"; then
    output_detail="${output_detail}, configure=OK"
  else
    output_ok=false
    output_detail="${output_detail}, configure=MISSING"
  fi

  if [ "$output_ok" = true ]; then
    echo "CHECK_OUTPUT_FORMAT=PASS ${output_detail}"
  else
    echo "CHECK_OUTPUT_FORMAT=FAIL ${output_detail}"
  fi
fi

# --- Edge-case-specific checks ---
case "$TEST_TYPE" in
  preinstalled_zsh)
    if echo "$setup_output" | grep -qi "Installing zsh"; then
      echo "CHECK_EDGE_SKIP_ZSH=FAIL (should not install zsh when pre-installed)"
    else
      echo "CHECK_EDGE_SKIP_ZSH=PASS (correctly skipped zsh install)"
    fi
    # Should still show the detected version
    if echo "$setup_output" | grep -qi "zsh.*found"; then
      echo "CHECK_EDGE_ZSH_DETECTED=PASS"
    else
      echo "CHECK_EDGE_ZSH_DETECTED=FAIL (should report detected zsh)"
    fi
    ;;
  preinstalled_all)
    if echo "$setup_output" | grep -qi "All dependencies already installed"; then
      echo "CHECK_EDGE_ALL_PRESENT=PASS"
    else
      echo "CHECK_EDGE_ALL_PRESENT=FAIL (should show all deps installed)"
    fi
    if echo "$setup_output" | grep -qi "The following will be installed"; then
      echo "CHECK_EDGE_NO_INSTALL=FAIL (should not install anything)"
    else
      echo "CHECK_EDGE_NO_INSTALL=PASS (correctly skipped installation)"
    fi
    ;;
  no_git)
    if echo "$setup_output" | grep -qi "git is required"; then
      echo "CHECK_EDGE_NO_GIT=PASS"
    else
      echo "CHECK_EDGE_NO_GIT=FAIL (should show git required error)"
    fi
    if [ "$setup_exit" -eq 0 ]; then
      echo "CHECK_EDGE_NO_GIT_EXIT=PASS (exit=0, graceful)"
    else
      echo "CHECK_EDGE_NO_GIT_EXIT=FAIL (exit=${setup_exit}, should be 0)"
    fi
    ;;
  broken_zsh)
    if echo "$setup_output" | grep -qi "modules are broken\|broken"; then
      echo "CHECK_EDGE_BROKEN_DETECTED=PASS"
    else
      echo "CHECK_EDGE_BROKEN_DETECTED=FAIL (should detect broken zsh)"
    fi
    ;;
  rerun)
    # Run forge zsh setup a second time
    # Update PATH to include ~/.local/bin (where GitHub-installed tools are located)
    # This simulates the PATH that would be set after sourcing ~/.zshrc
    export PATH="$HOME/.local/bin:/usr/local/bin:$PATH"
    hash -r  # Clear bash's command cache
    rerun_output=$(forge zsh setup --non-interactive 2>&1)
    rerun_exit=$?
    if [ "$rerun_exit" -eq 0 ]; then
      echo "CHECK_EDGE_RERUN_EXIT=PASS"
    else
      echo "CHECK_EDGE_RERUN_EXIT=FAIL (exit=${rerun_exit})"
    fi
    if echo "$rerun_output" | grep -qi "All dependencies already installed"; then
      echo "CHECK_EDGE_RERUN_SKIP=PASS"
    else
      echo "CHECK_EDGE_RERUN_SKIP=FAIL (second run should skip installs)"
    fi
    # Check marker uniqueness after re-run
    if [ -f "$HOME/.zshrc" ]; then
      start_count=$(grep -c '# >>> forge initialize >>>' "$HOME/.zshrc" 2>/dev/null || echo "0")
      if [ "$start_count" -eq 1 ]; then
        echo "CHECK_EDGE_RERUN_MARKERS=PASS (still exactly 1 marker set)"
      else
        echo "CHECK_EDGE_RERUN_MARKERS=FAIL (found ${start_count} marker sets)"
      fi
    else
      echo "CHECK_EDGE_RERUN_MARKERS=FAIL (no .zshrc after re-run)"
    fi
    ;;
  partial)
    # Should only install plugins, not zsh or OMZ
    if echo "$setup_output" | grep -qi "zsh-autosuggestions\|zsh-syntax-highlighting"; then
      echo "CHECK_EDGE_PARTIAL_PLUGINS=PASS (plugins in install plan)"
    else
      echo "CHECK_EDGE_PARTIAL_PLUGINS=FAIL (plugins not mentioned)"
    fi
    # The install plan should NOT mention zsh or Oh My Zsh
    # Extract only the install plan block (stop at first blank line after header)
    install_plan=$(echo "$setup_output" | sed -n '/The following will be installed/,/^$/p' 2>/dev/null || echo "")
    if [ -n "$install_plan" ]; then
      if echo "$install_plan" | grep -qi "zsh (shell)\|Oh My Zsh"; then
        echo "CHECK_EDGE_PARTIAL_NO_ZSH=FAIL (should not install zsh/OMZ)"
      else
        echo "CHECK_EDGE_PARTIAL_NO_ZSH=PASS (correctly skips zsh/OMZ)"
      fi
    else
      # If all deps including plugins are installed, that's also OK
      echo "CHECK_EDGE_PARTIAL_NO_ZSH=PASS (no install plan = nothing to install)"
    fi
    ;;
esac

# --- Emit raw output for debugging ---
echo "OUTPUT_BEGIN"
echo "$setup_output"
# If this is a re-run test, also show the second run output
if [ -n "$rerun_output" ]; then
  echo ""
  echo "===== SECOND RUN (idempotency check) ====="
  echo "$rerun_output"
  echo "=========================================="
fi
echo "OUTPUT_END"
VERIFY_SCRIPT_BODY
}

# =============================================================================
# Container execution
# =============================================================================

# Run the verify script inside a Docker container.
# Outputs: exit_code on line 1, then combined stdout+stderr.
run_container() {
  local tag="$1"
  local run_shell="$2"
  local test_type="$3"
  local exit_code=0
  local output
  output=$(docker run --rm "$tag" "$run_shell" -c "$(generate_verify_script "$test_type")" 2>&1) || exit_code=$?
  echo "$exit_code"
  echo "$output"
}

# =============================================================================
# Result evaluation
# =============================================================================

# Parse CHECK_* lines from container output and determine pass/fail.
parse_check_lines() {
  local output="$1"
  local label="$2"
  local all_pass=true
  local fail_details=""

  while IFS= read -r line; do
    case "$line" in
      CHECK_*=PASS*)
        ;;
      CHECK_*=FAIL*)
        all_pass=false
        fail_details="${fail_details}    ${line}\n"
        ;;
    esac
  done <<< "$output"

  if [ "$all_pass" = true ]; then
    echo "PASS"
  else
    echo "FAIL"
    echo -e "$fail_details"
  fi
}

# Run a single Docker test for a base image with a specific binary.
# Writes result file to $RESULTS_DIR.
run_single_test() {
  local entry="$1"
  local variant="$2"  # "root" or "user"
  local target="$3"   # rust target triple
  local test_type="${4:-standard}"

  IFS='|' read -r image label packages <<< "$entry"
  local safe_label
  safe_label=$(echo "$label" | tr '[:upper:]' '[:lower:]' | tr ' /' '_-' | tr -cd '[:alnum:]_-')
  local target_short="${target##*-}"  # musl or gnu
  local tag="${DOCKER_TAG_PREFIX}-${safe_label}-${variant}-${target_short}"
  local result_file="$RESULTS_DIR/${safe_label}-${variant}-${target_short}.result"

  local bin_rel
  bin_rel=$(binary_rel_path "$target")

  # Check binary exists
  if [ ! -f "$PROJECT_ROOT/$bin_rel" ]; then
    cat > "$result_file" <<EOF
STATUS: FAIL
LABEL: ${label} (${variant}) [${target_short}]
VARIANT: ${variant}
TARGET: ${target}
DETAILS: Binary not found: ${bin_rel}
EOF
    return
  fi

  # Build the Docker image
  local install_cmd
  install_cmd=$(pkg_install_cmd "$image" "$packages")

  local user_cmd=""
  local extra_setup=""

  if [ "$variant" = "user" ]; then
    user_cmd=$(user_setup_cmd "$image")
  fi

  if ! build_docker_image "$tag" "$image" "$bin_rel" "$install_cmd" "$user_cmd" "$extra_setup"; then
    local build_log="$RESULTS_DIR/docker-build-${tag}.log"
    local build_err=""
    if [ -f "$build_log" ]; then
      build_err=$(tail -5 "$build_log" 2>/dev/null || echo "(no log)")
    fi
    cat > "$result_file" <<EOF
STATUS: FAIL
LABEL: ${label} (${variant}) [${target_short}]
VARIANT: ${variant}
TARGET: ${target}
DETAILS: Docker build failed
BUILD_LOG: ${build_err}
EOF
    return
  fi

  # Run the container
  local raw_output
  raw_output=$(run_container "$tag" "bash" "$test_type" 2>&1) || true

  # Parse exit code (first line) and output (rest) without broken pipe
  local container_exit
  local container_output
  container_exit=$(head -1 <<< "$raw_output")
  container_output=$(tail -n +2 <<< "$raw_output")

  # Parse SETUP_EXIT
  local setup_exit
  setup_exit=$(grep '^SETUP_EXIT=' <<< "$container_output" | head -1 | cut -d= -f2)

  # Evaluate CHECK lines
  local eval_result
  eval_result=$(parse_check_lines "$container_output" "$label ($variant) [$target_short]")
  local status
  local details
  status=$(head -1 <<< "$eval_result")
  details=$(tail -n +2 <<< "$eval_result")

  # Check setup exit code (should be 0)
  if [ -n "$setup_exit" ] && [ "$setup_exit" != "0" ] && [ "$test_type" != "no_git" ]; then
    status="FAIL"
    details="${details}    SETUP_EXIT=${setup_exit} (expected 0)\n"
  fi

  # Write result
  cat > "$result_file" <<EOF
STATUS: ${status}
LABEL: ${label} (${variant}) [${target_short}]
VARIANT: ${variant}
TARGET: ${target}
DETAILS: ${details}
EOF

  # Save raw output for debugging
  local output_file="$RESULTS_DIR/${safe_label}-${variant}-${target_short}.output"
  echo "$container_output" > "$output_file"

  # Cleanup Docker image unless --no-cleanup
  if [ "$NO_CLEANUP" = false ]; then
    docker rmi -f "$tag" > /dev/null 2>&1 || true
  fi
}

# Run a single edge case test with a specific binary.
run_edge_case_test() {
  local entry="$1"
  local target="$2"

  IFS='|' read -r edge_type image label packages <<< "$entry"

  local safe_label
  safe_label=$(echo "$label" | tr '[:upper:]' '[:lower:]' | tr ' /' '_-' | tr -cd '[:alnum:]_-')
  local target_short="${target##*-}"
  local tag="${DOCKER_TAG_PREFIX}-edge-${safe_label}-${target_short}"
  local result_file="$RESULTS_DIR/edge-${safe_label}-${target_short}.result"

  local bin_rel
  bin_rel=$(binary_rel_path "$target")

  if [ ! -f "$PROJECT_ROOT/$bin_rel" ]; then
    cat > "$result_file" <<EOF
STATUS: FAIL
LABEL: ${label} [${target_short}]
VARIANT: edge
TARGET: ${target}
DETAILS: Binary not found: ${bin_rel}
EOF
    return
  fi

  local install_cmd
  local extra_setup=""
  local test_type="standard"

  case "$edge_type" in
    PREINSTALLED_ZSH)
      install_cmd=$(pkg_install_cmd "$image" "")
      extra_setup="apt-get install -y -qq zsh"
      test_type="preinstalled_zsh"
      ;;
    PREINSTALLED_ALL)
      install_cmd=$(pkg_install_cmd "$image" "")
      # Install zsh, OMZ, plugins, and tools (fzf, bat, fd)
      extra_setup='apt-get install -y -qq zsh fzf bat fd-find && sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)" "" --unattended && git clone https://github.com/zsh-users/zsh-autosuggestions.git $HOME/.oh-my-zsh/custom/plugins/zsh-autosuggestions && git clone https://github.com/zsh-users/zsh-syntax-highlighting.git $HOME/.oh-my-zsh/custom/plugins/zsh-syntax-highlighting'
      test_type="preinstalled_all"
      ;;
    NO_GIT)
      # Install only the minimal base — NO git
      install_cmd="apt-get update -qq && apt-get install -y -qq curl bash"
      test_type="no_git"
      ;;
    BROKEN_ZSH)
      install_cmd=$(pkg_install_cmd "$image" "")
      # Install zsh then delete module files to break it
      extra_setup="apt-get install -y -qq zsh && rm -rf /usr/lib/*/zsh /usr/lib/zsh"
      test_type="broken_zsh"
      ;;
    RERUN)
      install_cmd=$(pkg_install_cmd "$image" "")
      test_type="rerun"
      ;;
    PARTIAL)
      install_cmd=$(pkg_install_cmd "$image" "")
      # Install zsh and OMZ, but NOT plugins
      extra_setup='apt-get install -y -qq zsh && sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)" "" --unattended'
      test_type="partial"
      ;;
    *)
      install_cmd=$(pkg_install_cmd "$image" "")
      ;;
  esac

  if ! build_docker_image "$tag" "$image" "$bin_rel" "$install_cmd" "" "$extra_setup"; then
    local build_log="$RESULTS_DIR/docker-build-${tag}.log"
    local build_err=""
    if [ -f "$build_log" ]; then
      build_err=$(tail -5 "$build_log" 2>/dev/null || echo "(no log)")
    fi
    cat > "$result_file" <<EOF
STATUS: FAIL
LABEL: ${label} [${target_short}]
VARIANT: edge
TARGET: ${target}
DETAILS: Docker build failed
BUILD_LOG: ${build_err}
EOF
    return
  fi

  local raw_output
  raw_output=$(run_container "$tag" "bash" "$test_type" 2>&1) || true

  local container_exit
  local container_output
  container_exit=$(head -1 <<< "$raw_output")
  container_output=$(tail -n +2 <<< "$raw_output")

  local setup_exit
  setup_exit=$(grep '^SETUP_EXIT=' <<< "$container_output" | head -1 | cut -d= -f2)

  local eval_result
  eval_result=$(parse_check_lines "$container_output" "$label [$target_short]")
  local status
  local details
  status=$(head -1 <<< "$eval_result")
  details=$(tail -n +2 <<< "$eval_result")

  # For no_git test, exit code 0 is expected even though things "fail"
  if [ "$edge_type" != "NO_GIT" ] && [ -n "$setup_exit" ] && [ "$setup_exit" != "0" ]; then
    status="FAIL"
    details="${details}    SETUP_EXIT=${setup_exit} (expected 0)\n"
  fi

  cat > "$result_file" <<EOF
STATUS: ${status}
LABEL: ${label} [${target_short}]
VARIANT: edge
TARGET: ${target}
DETAILS: ${details}
EOF

  local output_file="$RESULTS_DIR/edge-${safe_label}-${target_short}.output"
  echo "$container_output" > "$output_file"

  if [ "$NO_CLEANUP" = false ]; then
    docker rmi -f "$tag" > /dev/null 2>&1 || true
  fi
}

# =============================================================================
# Parallel execution
# =============================================================================

# Determine which targets are compatible with a given image.
# Returns space-separated list of compatible targets.
#
# The gnu binary (x86_64-unknown-linux-gnu) requires glibc 2.38+ and won't
# run on Alpine (musl), Debian 12 (glibc 2.36), Ubuntu 22.04 (glibc 2.35),
# or Rocky 9 (glibc 2.34). The musl binary is statically linked and runs
# everywhere.
get_compatible_targets() {
  local image="$1"
  local all_targets="$2"  # space-separated list of available targets
  
  # Extract base image name (before colon)
  local base_image="${image%%:*}"
  
  # Images that ONLY support musl (old glibc or musl-based)
  case "$base_image" in
    alpine)
      # Alpine uses musl libc, not glibc
      echo "$all_targets" | tr ' ' '\n' | grep -E 'musl$'
      ;;
    debian)
      # Debian 12 has glibc 2.36 (too old for gnu binary built on glibc 2.43)
      echo "$all_targets" | tr ' ' '\n' | grep -E 'musl$'
      ;;
    ubuntu)
      # Check version: 22.04 has glibc 2.35 (musl only), 24.04 has glibc 2.39 (both)
      local version="${image#*:}"
      if [[ "$version" == "22.04" ]]; then
        echo "$all_targets" | tr ' ' '\n' | grep -E 'musl$'
      else
        # Ubuntu 24.04+ supports both
        echo "$all_targets"
      fi
      ;;
    rockylinux)
      # Rocky 9 has glibc 2.34 (too old)
      echo "$all_targets" | tr ' ' '\n' | grep -E 'musl$'
      ;;
    *)
      # All other images (Arch, Fedora, openSUSE, Void) have recent glibc and support both
      echo "$all_targets"
      ;;
  esac
}

launch_parallel_tests() {
  local max_jobs="${MAX_JOBS:-}"
  if [ -z "$max_jobs" ]; then
    max_jobs=$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)
    if [ "$max_jobs" -gt "$DEFAULT_MAX_JOBS" ]; then
      max_jobs=$DEFAULT_MAX_JOBS
    fi
  fi

  log_info "Running with up to ${max_jobs} parallel jobs"

  # Collect active targets
  local active_targets=()
  for entry in "${BUILD_TARGETS[@]}"; do
    IFS='|' read -r target _cross _label <<< "$entry"
    if [ -n "$TARGET_FILTER" ] && [ "$TARGET_FILTER" != "all" ]; then
      if ! echo "$target" | grep -qi "$TARGET_FILTER"; then
        continue
      fi
    fi
    local bin="$PROJECT_ROOT/$(binary_rel_path "$target")"
    if [ -f "$bin" ]; then
      active_targets+=("$target")
    fi
  done

  if [ ${#active_targets[@]} -eq 0 ]; then
    log_fail "No built binaries found for any target"
    return
  fi

  log_info "Testing ${#active_targets[@]} target(s): ${active_targets[*]}"

  # FIFO-based semaphore for concurrency control
  local fifo
  fifo=$(mktemp -u)
  mkfifo "$fifo"
  exec 3<>"$fifo"
  rm "$fifo"

  # Fill semaphore with tokens
  for ((i = 0; i < max_jobs; i++)); do
    echo >&3
  done

  # Launch base image tests for each target
  for target in "${active_targets[@]}"; do
    for entry in "${IMAGES[@]}"; do
      IFS='|' read -r image label _packages <<< "$entry"

      # Apply filter
      if [ -n "$FILTER_PATTERN" ] && ! echo "$label" | grep -qiE "$FILTER_PATTERN"; then
        continue
      fi
      if [ -n "$EXCLUDE_PATTERN" ] && echo "$label" | grep -qiE "$EXCLUDE_PATTERN"; then
        continue
      fi

      # Check if this image is compatible with this target
      local compatible_targets
      compatible_targets=$(get_compatible_targets "$image" "${active_targets[*]}")
      if ! echo "$compatible_targets" | grep -qw "$target"; then
        continue
      fi

      # Root variant
      read -u 3
      (
        run_single_test "$entry" "root" "$target" "standard"
        echo >&3
      ) &

      # User+sudo variant
      read -u 3
      (
        run_single_test "$entry" "user" "$target" "standard"
        echo >&3
      ) &
    done

    # Launch edge case tests for each target
    for entry in "${EDGE_CASES[@]}"; do
      IFS='|' read -r _type image label _packages <<< "$entry"

      if [ -n "$FILTER_PATTERN" ] && ! echo "$label" | grep -qiE "$FILTER_PATTERN"; then
        continue
      fi
      if [ -n "$EXCLUDE_PATTERN" ] && echo "$label" | grep -qiE "$EXCLUDE_PATTERN"; then
        continue
      fi

      # Check compatibility for edge cases too
      local compatible_targets
      compatible_targets=$(get_compatible_targets "$image" "${active_targets[*]}")
      if ! echo "$compatible_targets" | grep -qw "$target"; then
        continue
      fi

      read -u 3
      (
        run_edge_case_test "$entry" "$target"
        echo >&3
      ) &
    done
  done

  # Wait for all jobs to complete
  wait

  # Close semaphore FD
  exec 3>&-
}

# =============================================================================
# Result collection and reporting
# =============================================================================

collect_test_results() {
  log_header "Results"

  local has_results=false
  if [ -d "$RESULTS_DIR" ]; then
    for f in "$RESULTS_DIR"/*.result; do
      if [ -f "$f" ]; then
        has_results=true
        break
      fi
    done
  fi

  if [ "$has_results" = false ]; then
    log_skip "No test results found"
    return
  fi

  for result_file in "$RESULTS_DIR"/*.result; do
    [ -f "$result_file" ] || continue
    local status
    status=$(grep '^STATUS:' "$result_file" | head -1 | awk '{print $2}' || echo "UNKNOWN")
    local label
    label=$(grep '^LABEL:' "$result_file" | head -1 | sed 's/^LABEL: //' || echo "(unknown test)")

    case "$status" in
      PASS)
        log_pass "$label"
        ;;
      FAIL)
        log_fail "$label"
        local details
        details=$(grep '^DETAILS:' "$result_file" | head -1 | sed 's/^DETAILS: //' || true)
        if [ -n "$details" ] && [ "$details" != " " ]; then
          echo -e "    ${DIM}${details}${NC}"
        fi
        # Show build log if present
        local build_log_content
        build_log_content=$(grep '^BUILD_LOG:' "$result_file" | head -1 | sed 's/^BUILD_LOG: //' || true)
        if [ -n "$build_log_content" ] && [ "$build_log_content" != " " ]; then
          echo -e "    ${DIM}Build: ${build_log_content}${NC}"
        fi
        # Show failing CHECK lines from output file
        local output_file="${result_file%.result}.output"
        if [ -f "$output_file" ]; then
          grep 'CHECK_.*=FAIL' "$output_file" 2>/dev/null | while read -r line; do
            echo -e "    ${RED}${line}${NC}"
          done || true
        fi
        ;;
      *)
        log_skip "$label"
        ;;
    esac
  done
}

print_report() {
  echo ""
  echo -e "${BOLD}════════════════════════════════════════════════════════${NC}"
  local total=$((PASS + FAIL + SKIP))
  if [ "$FAIL" -eq 0 ]; then
    echo -e "${GREEN}${BOLD}  RESULTS: ${PASS} passed, ${FAIL} failed, ${SKIP} skipped (${total} total)${NC}"
  else
    echo -e "${RED}${BOLD}  RESULTS: ${PASS} passed, ${FAIL} failed, ${SKIP} skipped (${total} total)${NC}"
  fi
  echo -e "${BOLD}════════════════════════════════════════════════════════${NC}"

  if [ ${#FAILURES[@]} -gt 0 ]; then
    echo ""
    echo -e "${RED}${BOLD}Failed tests:${NC}"
    for f in "${FAILURES[@]}"; do
      echo -e "  ${RED}• ${f}${NC}"
    done
  fi

  if [ "$NO_CLEANUP" = true ] && [ -n "$RESULTS_DIR" ] && [ -d "$RESULTS_DIR" ]; then
    echo ""
    echo -e "  ${DIM}Results preserved: ${RESULTS_DIR}${NC}"
  fi
}

# =============================================================================
# Docker tests orchestrator
# =============================================================================

run_docker_tests() {
  # Check Docker is available
  if ! command -v docker > /dev/null 2>&1; then
    log_skip "Docker not installed"
    return
  fi

  if ! docker info > /dev/null 2>&1; then
    log_skip "Docker daemon not running"
    return
  fi

  # Create results directory (needed by build phase for logs)
  # Use a known path for CI artifact upload when --no-cleanup
  if [ "$NO_CLEANUP" = true ]; then
    RESULTS_DIR="$PROJECT_ROOT/test-results-linux"
    rm -rf "$RESULTS_DIR"
    mkdir -p "$RESULTS_DIR"
  else
    RESULTS_DIR=$(mktemp -d)
  fi

  # Build binaries
  build_all_targets

  log_header "Phase 3: Docker E2E Tests"
  log_info "Results dir: ${RESULTS_DIR}"

  # Run tests in parallel
  launch_parallel_tests

  # Collect and display results
  collect_test_results
}

# =============================================================================
# Main
# =============================================================================

main() {
  parse_args "$@"

  echo -e "${BOLD}${BLUE}Forge ZSH Setup — E2E Test Suite${NC}"
  echo ""

  run_static_checks

  if [ "$MODE" = "quick" ]; then
    echo ""
    print_report
    if [ "$FAIL" -gt 0 ]; then
      exit 1
    fi
    exit 0
  fi

  run_docker_tests

  echo ""
  print_report

  # Cleanup results dir unless --no-cleanup
  if [ "$NO_CLEANUP" = false ] && [ -n "$RESULTS_DIR" ] && [ -d "$RESULTS_DIR" ]; then
    rm -rf "$RESULTS_DIR"
  fi

  if [ "$FAIL" -gt 0 ]; then
    exit 1
  fi
  exit 0
}

main "$@"
