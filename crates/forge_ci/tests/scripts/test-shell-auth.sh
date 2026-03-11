#!/bin/bash
# =============================================================================
# E2E test suite for shell-native provider authentication
#
# Tests `forge provider auth-info` and `forge provider login` non-interactive
# CLI commands, and the `_forge_provider_auth` zsh shell function.
#
# This replaces the crossterm/dialoguer interactive selection (which crashes on
# Windows Git Bash mintty with "IO error: Incorrect function (os error 1)") with
# shell-native fzf/read prompts.
#
# Three execution modes:
#
#   Docker (Linux, default):
#     Builds musl + gnu binaries then runs tests inside Docker containers across
#     multiple distros (Ubuntu, Debian, Fedora, Rocky, Alpine, Arch, openSUSE, Void).
#     Build targets (arch-aware):
#       x86_64:  x86_64-unknown-linux-musl, x86_64-unknown-linux-gnu
#       aarch64: aarch64-unknown-linux-musl, aarch64-unknown-linux-gnu
#
#   Native (macOS / Windows, --native):
#     Builds a single host binary and runs the full verification script directly
#     on the host. macOS gets CLI + zsh function tests. Windows gets CLI only
#     (use --skip-zsh since zsh is not available on Windows runners).
#
#   Quick (--quick):
#     Static analysis only (bash -n + shellcheck). No build, no Docker, no binary.
#
# Usage:
#   bash crates/forge_ci/tests/scripts/test-shell-auth.sh                    # Linux Docker
#   bash crates/forge_ci/tests/scripts/test-shell-auth.sh --native           # macOS/Windows
#   bash crates/forge_ci/tests/scripts/test-shell-auth.sh --native --skip-zsh  # Windows
#   bash crates/forge_ci/tests/scripts/test-shell-auth.sh --skip-build       # reuse binaries
#   bash crates/forge_ci/tests/scripts/test-shell-auth.sh --native-build     # cargo not cross
#   bash crates/forge_ci/tests/scripts/test-shell-auth.sh --filter "alpine"  # Docker only
#   bash crates/forge_ci/tests/scripts/test-shell-auth.sh --jobs 4           # Docker only
#   bash crates/forge_ci/tests/scripts/test-shell-auth.sh --targets musl     # Docker only
#   bash crates/forge_ci/tests/scripts/test-shell-auth.sh --list
#   bash crates/forge_ci/tests/scripts/test-shell-auth.sh --quick
#   bash crates/forge_ci/tests/scripts/test-shell-auth.sh --help
#
# Test result format (matches test-zsh-setup.sh):
#   CHECK_<NAME>=PASS [detail]
#   CHECK_<NAME>=FAIL [detail]
#
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
readonly DOCKER_TAG_PREFIX="forge-auth-test"
readonly DEFAULT_MAX_JOBS=8

# Detect host architecture
HOST_ARCH="$(uname -m)"
readonly HOST_ARCH

# Build targets — matches CI release.yml for Linux, arch-aware
# Format: "target|cross_flag|label"
if [ "$HOST_ARCH" = "aarch64" ] || [ "$HOST_ARCH" = "arm64" ]; then
  readonly BUILD_TARGETS=(
    "aarch64-unknown-linux-musl|true|musl (static)"
    "aarch64-unknown-linux-gnu|false|gnu (dynamic)"
  )
elif [ "$HOST_ARCH" = "x86_64" ] || [ "$HOST_ARCH" = "amd64" ]; then
  readonly BUILD_TARGETS=(
    "x86_64-unknown-linux-musl|true|musl (static)"
    "x86_64-unknown-linux-gnu|false|gnu (dynamic)"
  )
else
  echo "Error: Unsupported host architecture: $HOST_ARCH" >&2
  echo "Supported: x86_64, amd64, aarch64, arm64" >&2
  exit 1
fi

# Docker images to test against
# Format: "image|label"
readonly IMAGES=(
  # --- Tier 1: apt-get (Debian/Ubuntu) ---
  "ubuntu:24.04|Ubuntu 24.04 (apt-get)"
  "ubuntu:22.04|Ubuntu 22.04 (apt-get)"
  "debian:bookworm-slim|Debian 12 Slim (apt-get)"

  # --- Tier 2: dnf (Fedora/RHEL) ---
  "fedora:41|Fedora 41 (dnf)"
  "rockylinux:9|Rocky Linux 9 (dnf)"

  # --- Tier 3: apk (Alpine) ---
  "alpine:3.20|Alpine 3.20 (apk)"

  # --- Tier 4: pacman (Arch) ---
  "archlinux:latest|Arch Linux (pacman)"

  # --- Tier 5: zypper (openSUSE) ---
  "opensuse/tumbleweed:latest|openSUSE Tumbleweed (zypper)"

  # --- Tier 6: xbps (Void) ---
  "ghcr.io/void-linux/void-glibc:latest|Void Linux glibc (xbps)"
)

# =============================================================================
# Runtime state
# =============================================================================

PASS=0
FAIL=0
SKIP=0
FAILURES=()

# CLI options
MODE="full"          # full | quick (shellcheck only) | native (run on host, no Docker)
MAX_JOBS=""
FILTER_PATTERN=""
EXCLUDE_PATTERN=""
NO_CLEANUP=false
SKIP_BUILD=false
TARGET_FILTER=""     # empty = all, "musl" or "gnu" to filter
NATIVE_BUILD=false   # if true, use cargo instead of cross

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
Usage: bash crates/forge_ci/tests/scripts/test-shell-auth.sh [OPTIONS]

Options:
  --quick              Run static analysis only (no Docker, no native tests)
  --native             Run tests directly on host (no Docker, for macOS/Windows CI)
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
      --quick)        MODE="quick"; shift ;;
      --native)       MODE="native"; shift ;;
      --jobs)         MAX_JOBS="${2:?--jobs requires a number}"; shift 2 ;;
      --filter)       FILTER_PATTERN="${2:?--filter requires a pattern}"; shift 2 ;;
      --exclude)      EXCLUDE_PATTERN="${2:?--exclude requires a pattern}"; shift 2 ;;
      --skip-build)   SKIP_BUILD=true; shift ;;
      --targets)      TARGET_FILTER="${2:?--targets requires a value}"; shift 2 ;;
      --native-build) NATIVE_BUILD=true; shift ;;
      --no-cleanup)   NO_CLEANUP=true; shift ;;
      --list)         list_images; exit 0 ;;
      --help|-h)      print_usage; exit 0 ;;
      *)              echo "Unknown option: $1" >&2; print_usage >&2; exit 1 ;;
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
    printf "  %2d. %-45s %s\n" "$idx" "$label" "$target"
  done

  echo -e "\n${BOLD}Docker Images:${NC}"
  for entry in "${IMAGES[@]}"; do
    idx=$((idx + 1))
    IFS='|' read -r image label <<< "$entry"
    printf "  %2d. %-45s %s\n" "$idx" "$label" "$image"
  done
}

# =============================================================================
# Build binaries
# =============================================================================

build_binary() {
  local target="$1"
  local use_cross="$2"
  local binary_path="$PROJECT_ROOT/target/${target}/debug/forge"

  if [ "$SKIP_BUILD" = true ] && [ -f "$binary_path" ]; then
    log_info "Skipping build for ${target} (binary exists)"
    return 0
  fi

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
      cat "$RESULTS_DIR/build-${target}.log" >&2
      return 1
    fi
  else
    if ! rustup target list --installed 2>/dev/null | grep -q "$target"; then
      log_info "Adding Rust target ${target}..."
      rustup target add "$target" 2>/dev/null || true
    fi
    log_info "Building ${target} with cargo (debug)..."
    if ! cargo build --target "$target" 2>"$RESULTS_DIR/build-${target}.log"; then
      log_fail "Build failed for ${target}"
      cat "$RESULTS_DIR/build-${target}.log" >&2
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

build_all_targets() {
  log_header "Phase 1: Build Binaries"

  for entry in "${BUILD_TARGETS[@]}"; do
    IFS='|' read -r target use_cross label <<< "$entry"

    if [ -n "$TARGET_FILTER" ] && [ "$TARGET_FILTER" != "all" ]; then
      if ! echo "$target" | grep -qi "$TARGET_FILTER"; then
        log_skip "${label} (filtered out by --targets ${TARGET_FILTER})"
        continue
      fi
    fi

    if ! build_binary "$target" "$use_cross"; then
      echo "Error: Build failed for ${target}. Cannot continue without binaries." >&2
      exit 1
    fi
  done
}

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

# Returns the package manager install command for a given image.
# Installs: zsh, git, curl, bash (what's needed to run tests).
pkg_install_cmd() {
  local image="$1"
  case "$image" in
    alpine*)
      echo "apk add --no-cache zsh git curl bash fzf fd bat"
      ;;
    fedora*)
      echo "dnf install -y zsh git curl fzf fd-find bat"
      ;;
    rockylinux*|almalinux*|centos*)
      # EPEL provides fzf, fd-find, bat on RHEL-based distros
      echo "dnf install -y epel-release && dnf install -y zsh git curl fzf fd-find bat"
      ;;
    archlinux*)
      echo "pacman -Sy --noconfirm zsh git curl fzf fd bat"
      ;;
    opensuse*|suse*)
      echo "zypper -n install zsh git curl fzf fd bat"
      ;;
    *void*)
      echo "xbps-install -Sy zsh git curl bash fzf fd bat"
      ;;
    *)
      # Debian/Ubuntu: fd binary is 'fdfind', bat binary is 'batcat' on older releases.
      # On Ubuntu 22.04+ and Debian 12+ both ship as their canonical names.
      echo "apt-get update -qq && apt-get install -y -qq zsh git curl fzf fd-find bat"
      ;;
  esac
}

# =============================================================================
# In-container verification script
# =============================================================================

# Generates the bash+zsh script that runs inside the Docker container.
# Uses single-quoted heredoc so no host-side variable expansion occurs.
generate_verify_script() {
  cat <<'VERIFY_SCRIPT'
#!/bin/bash
set -o pipefail

# =============================================================================
# Phase A: CLI auth-info tests
# =============================================================================

echo ""
echo "=== Phase A: CLI auth-info ==="

# A1: auth-info for anthropic — check output fields
output=$(forge provider auth-info anthropic 2>&1) || true

if echo "$output" | grep -q "^auth_methods="; then
  echo "CHECK_AUTH_INFO_HAS_AUTH_METHODS=PASS $(echo "$output" | grep "^auth_methods=")"
else
  echo "CHECK_AUTH_INFO_HAS_AUTH_METHODS=FAIL missing auth_methods line in: $output"
fi

if echo "$output" | grep -q "^url_params="; then
  echo "CHECK_AUTH_INFO_HAS_URL_PARAMS=PASS $(echo "$output" | grep "^url_params=")"
else
  echo "CHECK_AUTH_INFO_HAS_URL_PARAMS=FAIL missing url_params line in: $output"
fi

if echo "$output" | grep -q "^configured="; then
  echo "CHECK_AUTH_INFO_HAS_CONFIGURED=PASS $(echo "$output" | grep "^configured=")"
else
  echo "CHECK_AUTH_INFO_HAS_CONFIGURED=FAIL missing configured line in: $output"
fi

# A2: anthropic uses api_key auth method
if echo "$output" | grep -q "^auth_methods=api_key"; then
  echo "CHECK_AUTH_INFO_ANTHROPIC_API_KEY=PASS auth_methods=api_key"
else
  echo "CHECK_AUTH_INFO_ANTHROPIC_API_KEY=FAIL expected api_key, got: $(echo "$output" | grep "^auth_methods=" || echo "not found")"
fi

# A3: anthropic has no url_params
if echo "$output" | grep -q "^url_params=$"; then
  echo "CHECK_AUTH_INFO_ANTHROPIC_NO_URL_PARAMS=PASS url_params empty"
else
  echo "CHECK_AUTH_INFO_ANTHROPIC_NO_URL_PARAMS=FAIL expected empty url_params"
fi

# A4: unknown provider outputs error message (exit 0 is the forge error pattern)
unknown_output=$(forge provider auth-info nonexistent-provider-xyz-abc 2>&1) || true
if echo "$unknown_output" | grep -qi "not found\|error\|unknown"; then
  echo "CHECK_AUTH_INFO_UNKNOWN_PROVIDER_ERROR=PASS error message present"
else
  echo "CHECK_AUTH_INFO_UNKNOWN_PROVIDER_ERROR=FAIL expected error message, got: $unknown_output"
fi

# A5: configured=no after logout
forge provider logout anthropic 2>/dev/null || true
info_after_logout=$(forge provider auth-info anthropic 2>&1) || true
if echo "$info_after_logout" | grep -q "^configured=no"; then
  echo "CHECK_AUTH_INFO_CONFIGURED_NO_AFTER_LOGOUT=PASS configured=no"
else
  echo "CHECK_AUTH_INFO_CONFIGURED_NO_AFTER_LOGOUT=FAIL expected configured=no, got: $info_after_logout"
fi

# =============================================================================
# Phase B: CLI non-interactive login tests
# =============================================================================

echo ""
echo "=== Phase B: CLI non-interactive login ==="

# Ensure clean state
forge provider logout anthropic 2>/dev/null || true

# B1: login with --init-only (no model fetch, no terminal needed)
login_output=$(forge provider login anthropic \
  --auth-method api-key \
  --api-key "sk-ant-test-key-for-ci-testing" \
  --init-only 2>&1) || true

if echo "$login_output" | grep -qi "configured successfully\|Anthropic configured"; then
  echo "CHECK_LOGIN_INIT_ONLY_SUCCESS_MSG=PASS got success message"
else
  echo "CHECK_LOGIN_INIT_ONLY_SUCCESS_MSG=FAIL no success message: $login_output"
fi

# B2: configured=yes after login
info_after_login=$(forge provider auth-info anthropic 2>&1) || true
if echo "$info_after_login" | grep -q "^configured=yes"; then
  echo "CHECK_LOGIN_CONFIGURED_YES=PASS configured=yes"
else
  echo "CHECK_LOGIN_CONFIGURED_YES=FAIL expected configured=yes, got: $info_after_login"
fi

# B3: --set-active flag sets provider as active (no terminal prompt)
set_active_output=$(forge provider login anthropic \
  --auth-method api-key \
  --api-key "sk-ant-test-key-for-ci-testing" \
  --set-active \
  --init-only 2>&1) || true

if echo "$set_active_output" | grep -qi "default provider\|now the default\|set as active\|configured successfully"; then
  echo "CHECK_LOGIN_SET_ACTIVE_MSG=PASS got activation message"
else
  echo "CHECK_LOGIN_SET_ACTIVE_MSG=FAIL no activation message: $set_active_output"
fi

# B4: no Windows mintty crossterm errors (core regression test)
# "IO error: Incorrect function (os error 1)" from BracketedPasteGuard::new()
if echo "$login_output$set_active_output" | grep -qi "incorrect function\|bracketedpaste\|os error 1"; then
  echo "CHECK_LOGIN_NO_CROSSTERM_ERRORS=FAIL found Windows mintty crossterm errors"
else
  echo "CHECK_LOGIN_NO_CROSSTERM_ERRORS=PASS no Windows mintty crossterm errors"
fi

# B5: invalid auth method returns non-zero exit
invalid_exit=0
forge provider login anthropic \
  --auth-method invalid-method-xyz \
  --api-key "sk-test" \
  --init-only 2>/dev/null || invalid_exit=$?
if [ "$invalid_exit" -ne 0 ]; then
  echo "CHECK_LOGIN_INVALID_AUTH_METHOD_ERROR=PASS exit=$invalid_exit"
else
  echo "CHECK_LOGIN_INVALID_AUTH_METHOD_ERROR=FAIL expected non-zero exit"
fi

# B6: auth-info works without a TTY (redirect stdin from /dev/null)
notty_output=$(forge provider auth-info anthropic 2>&1 </dev/null) || true
if echo "$notty_output" | grep -qi "incorrect function\|bracketedpaste\|os error 1"; then
  echo "CHECK_AUTH_INFO_NO_TTY=FAIL BracketedPaste error without TTY"
else
  echo "CHECK_AUTH_INFO_NO_TTY=PASS no BracketedPaste errors without TTY"
fi

# Cleanup
forge provider logout anthropic 2>/dev/null || true

# B7: configured=no after logout
info_after_logout2=$(forge provider auth-info anthropic 2>&1) || true
if echo "$info_after_logout2" | grep -q "^configured=no"; then
  echo "CHECK_LOGOUT_CONFIGURED_NO=PASS configured=no after logout"
else
  echo "CHECK_LOGOUT_CONFIGURED_NO=FAIL expected configured=no, got: $info_after_logout2"
fi

# =============================================================================
# Phase C: ZSH shell function tests
# =============================================================================

echo ""
echo "=== Phase C: ZSH shell function tests ==="

if ! command -v zsh > /dev/null 2>&1; then
  echo "CHECK_ZSH_AVAILABLE=FAIL zsh not found — install zsh before running tests"
  exit 1
fi
echo "CHECK_ZSH_AVAILABLE=PASS $(zsh --version 2>&1 | head -1)"

# Write the zsh test script to a temp file.
# zsh behaves differently when run as a script file vs receiving stdin:
# compinit and eval work correctly in file mode.
ZSH_SCRIPT=$(mktemp /tmp/forge_zsh_test_XXXXXX.zsh)
cat > "$ZSH_SCRIPT" <<'ZSH_TEST_SCRIPT'
#!/usr/bin/env zsh

# Initialize zsh completion system so compdef works (needed by forge zsh plugin)
autoload -Uz compinit 2>/dev/null
compinit -u 2>/dev/null || true

# Source the forge zsh plugin
eval "$(forge zsh plugin 2>/dev/null)" 2>/dev/null
if [[ -z "$_FORGE_PLUGIN_LOADED" ]]; then
  echo "CHECK_ZSH_PLUGIN_LOAD=FAIL _FORGE_PLUGIN_LOADED not set after eval"
  exit 1
fi
echo "CHECK_ZSH_PLUGIN_LOAD=PASS plugin loaded (timestamp=$_FORGE_PLUGIN_LOADED)"

# Mock fzf: always selects the first line of input
function fzf() {
  head -1
}

# Capture directory for _forge_exec calls
CAPTURE_DIR="$(mktemp -d)"
CALL_COUNT=0

# Mock _forge_exec: captures args to file, returns pre-canned auth-info output.
# We do NOT forward auth-info to the real binary because the real binary
# consumes stdin (for REPL mode), which would steal the piped API key input
# before 'read -rs' can read it.
function _forge_exec() {
  CALL_COUNT=$((CALL_COUNT + 1))
  echo "$@" > "${CAPTURE_DIR}/call_${CALL_COUNT}.txt"

  if [[ "$1" == "provider" && "$2" == "auth-info" && "$3" == "anthropic" ]]; then
    echo "auth_methods=api_key"
    echo "url_params="
    echo "configured=no"
    return 0
  fi

  if [[ "$1" == "provider" && "$2" == "login" ]]; then
    echo "configured successfully (mocked)"
    return 0
  fi

  return 0
}

# Ensure anthropic is logged out for clean state
forge provider logout anthropic 2>/dev/null || true

# Reset capture
CALL_COUNT=0
rm -f "${CAPTURE_DIR}"/call_*.txt

# Simulate user input: API key via stdin pipe.
# _forge_provider_auth reads the API key with 'read -rs' from stdin.
(
  echo "sk-ant-test-key-from-shell"
) | _forge_provider_auth "anthropic" 2>/dev/null
auth_exit=$?

# Find the login call (should be call #2 after auth-info)
login_call_file=""
for f in "${CAPTURE_DIR}"/call_*.txt; do
  if grep -q "provider login" "$f" 2>/dev/null; then
    login_call_file="$f"
    break
  fi
done

if [[ -n "$login_call_file" ]]; then
  login_args="$(cat "$login_call_file")"

  if echo "$login_args" | grep -q "\-\-auth-method api-key"; then
    echo "CHECK_ZSH_AUTH_METHOD_ARG=PASS --auth-method api-key present"
  else
    echo "CHECK_ZSH_AUTH_METHOD_ARG=FAIL missing --auth-method api-key in: $login_args"
  fi

  if echo "$login_args" | grep -q "\-\-api-key"; then
    echo "CHECK_ZSH_API_KEY_ARG=PASS --api-key present"
  else
    echo "CHECK_ZSH_API_KEY_ARG=FAIL missing --api-key in: $login_args"
  fi

  if echo "$login_args" | grep -q "\-\-set-active"; then
    echo "CHECK_ZSH_SET_ACTIVE_ARG=PASS --set-active present"
  else
    echo "CHECK_ZSH_SET_ACTIVE_ARG=FAIL missing --set-active in: $login_args"
  fi

  if echo "$login_args" | grep -q "anthropic"; then
    echo "CHECK_ZSH_PROVIDER_ID_ARG=PASS provider id anthropic present"
  else
    echo "CHECK_ZSH_PROVIDER_ID_ARG=FAIL missing provider id in: $login_args"
  fi

  # Auth method must be kebab-case (api-key), not underscore (api_key)
  if echo "$login_args" | grep -q "\-\-auth-method api-key" && ! echo "$login_args" | grep -q "\-\-auth-method api_key"; then
    echo "CHECK_ZSH_KEBAB_CASE=PASS api-key (not api_key)"
  else
    echo "CHECK_ZSH_KEBAB_CASE=FAIL wrong format in: $login_args"
  fi
else
  echo "CHECK_ZSH_AUTH_METHOD_ARG=FAIL no login call captured"
  echo "CHECK_ZSH_API_KEY_ARG=FAIL no login call captured"
  echo "CHECK_ZSH_SET_ACTIVE_ARG=FAIL no login call captured"
  echo "CHECK_ZSH_PROVIDER_ID_ARG=FAIL no login call captured"
  echo "CHECK_ZSH_KEBAB_CASE=FAIL no login call captured"
fi

# Test _forge_action_login: selects provider via fzf then calls _forge_provider_auth
CALL_COUNT=0
rm -f "${CAPTURE_DIR}"/call_*.txt

function _forge_select_provider() {
  echo "Anthropic                  anthropic                    [empty]            llm"
}

(echo "sk-ant-action-login-test") | _forge_action_login "" 2>/dev/null
action_exit=$?

login_found=false
for f in "${CAPTURE_DIR}"/call_*.txt; do
  if grep -q "provider login" "$f" 2>/dev/null; then
    login_found=true
    break
  fi
done

if [[ "$login_found" == "true" ]]; then
  echo "CHECK_ZSH_ACTION_LOGIN_CALLS_PROVIDER_AUTH=PASS login call captured"
else
  echo "CHECK_ZSH_ACTION_LOGIN_CALLS_PROVIDER_AUTH=FAIL no login call captured"
fi

rm -rf "${CAPTURE_DIR}"
ZSH_TEST_SCRIPT

chmod +x "$ZSH_SCRIPT"
zsh_output=$(zsh "$ZSH_SCRIPT" 2>&1) || true
rm -f "$ZSH_SCRIPT"

echo "$zsh_output"
VERIFY_SCRIPT
}

# =============================================================================
# Container execution
# =============================================================================

run_container() {
  local tag="$1"
  local exit_code=0
  local output
  output=$(docker run --rm "$tag" bash -c "$(generate_verify_script)" 2>&1) || exit_code=$?
  echo "$exit_code"
  echo "$output"
}

# =============================================================================
# Result evaluation
# =============================================================================

parse_check_lines() {
  local output="$1"
  local all_pass=true
  local fail_details=""

  while IFS= read -r line; do
    case "$line" in
      CHECK_*=PASS*) ;;
      CHECK_*=SKIP*) ;;
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

# Determine which targets are compatible with a given image.
# The gnu binary requires glibc 2.38+ and won't run on Alpine (musl),
# Debian 12 (glibc 2.36), Ubuntu 22.04 (glibc 2.35), or Rocky 9 (glibc 2.34).
# The musl binary is statically linked and runs everywhere.
get_compatible_targets() {
  local image="$1"
  local all_targets="$2"  # space-separated list

  local base_image="${image%%:*}"

  case "$base_image" in
    alpine)
      echo "$all_targets" | tr ' ' '\n' | grep -E 'musl$'
      ;;
    debian)
      echo "$all_targets" | tr ' ' '\n' | grep -E 'musl$'
      ;;
    ubuntu)
      local version="${image#*:}"
      if [[ "$version" == "22.04" ]]; then
        echo "$all_targets" | tr ' ' '\n' | grep -E 'musl$'
      else
        echo "$all_targets"
      fi
      ;;
    rockylinux)
      echo "$all_targets" | tr ' ' '\n' | grep -E 'musl$'
      ;;
    *)
      # Arch, Fedora, openSUSE, Void — all have recent glibc, support both
      echo "$all_targets"
      ;;
  esac
}

# Run a single Docker test for an image + target combination.
# Writes result file to $RESULTS_DIR.
run_single_test() {
  local entry="$1"
  local target="$2"

  IFS='|' read -r image label <<< "$entry"
  local safe_label
  safe_label=$(echo "$label" | tr '[:upper:]' '[:lower:]' | tr ' /' '_-' | tr -cd '[:alnum:]_-')
  local target_short="${target##*-}"  # musl or gnu
  local tag="${DOCKER_TAG_PREFIX}-${safe_label}-${target_short}"
  local result_file="$RESULTS_DIR/${safe_label}-${target_short}.result"

  local bin_rel
  bin_rel=$(binary_rel_path "$target")

  if [ ! -f "$PROJECT_ROOT/$bin_rel" ]; then
    cat > "$result_file" <<EOF
STATUS: FAIL
LABEL: ${label} [${target_short}]
TARGET: ${target}
DETAILS: Binary not found: ${bin_rel}
EOF
    return
  fi

  local install_cmd
  install_cmd=$(pkg_install_cmd "$image")

  local build_log="$RESULTS_DIR/docker-build-${tag}.log"
  if ! docker build --quiet -t "$tag" -f - "$PROJECT_ROOT" <<DOCKERFILE >"$build_log" 2>&1
FROM ${image}
ENV DEBIAN_FRONTEND=noninteractive
ENV TERM=dumb
ENV NO_COLOR=1
RUN ${install_cmd}
COPY ${bin_rel} /usr/local/bin/forge
RUN chmod +x /usr/local/bin/forge
DOCKERFILE
  then
    local build_err
    build_err=$(tail -5 "$build_log" 2>/dev/null || echo "(no log)")
    cat > "$result_file" <<EOF
STATUS: FAIL
LABEL: ${label} [${target_short}]
TARGET: ${target}
DETAILS: Docker build failed
BUILD_LOG: ${build_err}
EOF
    return
  fi

  local raw_output
  raw_output=$(run_container "$tag" 2>&1) || true

  local container_exit
  local container_output
  container_exit=$(head -1 <<< "$raw_output")
  container_output=$(tail -n +2 <<< "$raw_output")

  local eval_result
  eval_result=$(parse_check_lines "$container_output")
  local status
  local details
  status=$(head -1 <<< "$eval_result")
  details=$(tail -n +2 <<< "$eval_result")

  cat > "$result_file" <<EOF
STATUS: ${status}
LABEL: ${label} [${target_short}]
TARGET: ${target}
DETAILS: ${details}
EOF

  local output_file="$RESULTS_DIR/${safe_label}-${target_short}.output"
  echo "$container_output" > "$output_file"

  if [ "$NO_CLEANUP" = false ]; then
    docker rmi -f "$tag" > /dev/null 2>&1 || true
  fi
}

# =============================================================================
# Parallel execution (FIFO job queue — matches test-zsh-setup.sh pattern)
# =============================================================================

launch_parallel_tests() {
  local max_jobs="${MAX_JOBS:-}"
  if [ -z "$max_jobs" ]; then
    max_jobs=$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)
    if [ "$max_jobs" -gt "$DEFAULT_MAX_JOBS" ]; then
      max_jobs=$DEFAULT_MAX_JOBS
    fi
  fi

  log_info "Running with up to ${max_jobs} parallel jobs"

  # Collect active targets (built binaries only)
  local active_targets=()
  for entry in "${BUILD_TARGETS[@]}"; do
    IFS='|' read -r target _cross _label <<< "$entry"
    if [ -n "$TARGET_FILTER" ] && [ "$TARGET_FILTER" != "all" ]; then
      if ! echo "$target" | grep -qi "$TARGET_FILTER"; then
        continue
      fi
    fi
    if [ -f "$PROJECT_ROOT/$(binary_rel_path "$target")" ]; then
      active_targets+=("$target")
    fi
  done

  if [ ${#active_targets[@]} -eq 0 ]; then
    log_skip "No built binaries found — skipping Docker tests"
    return 0
  fi

  local all_targets_str
  all_targets_str=$(printf '%s\n' "${active_targets[@]}" | tr '\n' ' ')

  # Build test queue: image x compatible targets
  local test_queue=()
  for entry in "${IMAGES[@]}"; do
    IFS='|' read -r image label <<< "$entry"

    if [ -n "$FILTER_PATTERN" ] && ! echo "$label" | grep -qiE "$FILTER_PATTERN"; then
      continue
    fi
    if [ -n "$EXCLUDE_PATTERN" ] && echo "$label" | grep -qiE "$EXCLUDE_PATTERN"; then
      continue
    fi

    local compatible_targets
    compatible_targets=$(get_compatible_targets "$image" "$all_targets_str")

    while IFS= read -r target; do
      [ -z "$target" ] && continue
      test_queue+=("${entry}:::${target}")
    done <<< "$compatible_targets"
  done

  if [ ${#test_queue[@]} -eq 0 ]; then
    log_skip "No tests to run (all filtered out)"
    return 0
  fi

  log_info "Queued ${#test_queue[@]} test combinations"

  # FIFO semaphore for parallel job limiting
  local fifo="$RESULTS_DIR/job_fifo"
  mkfifo "$fifo"
  exec 9<>"$fifo"
  rm -f "$fifo"

  # Pre-fill the semaphore with tokens
  local i
  for i in $(seq 1 "$max_jobs"); do
    echo >&9
  done

  local pids=()
  for item in "${test_queue[@]}"; do
    local entry target
    entry="${item%:::*}"
    target="${item#*:::}"

    # Acquire a token (blocks if max_jobs already running)
    read -r -u 9

    (
      run_single_test "$entry" "$target"
      echo >&9  # Release token when done
    ) &
    pids+=($!)
  done

  # Wait for all background jobs
  for pid in "${pids[@]}"; do
    wait "$pid" 2>/dev/null || true
  done

  exec 9>&-
}

# =============================================================================
# Collect and display results
# =============================================================================

collect_results() {
  log_header "Phase 3: Results"

  local found_any=false
  for result_file in "$RESULTS_DIR"/*.result; do
    [ -f "$result_file" ] || continue
    found_any=true

    local status label details
    status=$(grep "^STATUS:" "$result_file" | cut -d' ' -f2)
    label=$(grep "^LABEL:" "$result_file" | cut -d' ' -f2-)
    details=$(grep "^DETAILS:" "$result_file" | cut -d' ' -f2- || true)

    if [ "$status" = "PASS" ]; then
      log_pass "$label"
    else
      log_fail "$label"
      if [ -n "$details" ]; then
        log_info "  $details"
      fi
      # Show failing CHECK lines from the output file
      local output_file="${result_file%.result}.output"
      if [ -f "$output_file" ]; then
        grep "^CHECK_.*=FAIL" "$output_file" 2>/dev/null | while IFS= read -r line; do
          log_info "    $line"
        done
      fi
    fi
  done

  if [ "$found_any" = false ]; then
    log_skip "No result files found"
  fi
}

# =============================================================================
# Result summary
# =============================================================================

print_summary() {
  log_header "Test Summary"
  echo ""
  echo -e "  ${GREEN}PASS: $PASS${NC}"
  echo -e "  ${RED}FAIL: $FAIL${NC}"
  echo -e "  ${YELLOW}SKIP: $SKIP${NC}"
  echo ""

  if [ ${#FAILURES[@]} -gt 0 ]; then
    echo -e "${RED}${BOLD}Failed tests:${NC}"
    for f in "${FAILURES[@]}"; do
      echo -e "  ${RED}x${NC} $f"
    done
    echo ""
  fi

  if [ "$FAIL" -eq 0 ]; then
    echo -e "${GREEN}${BOLD}All tests passed!${NC}"
    return 0
  else
    echo -e "${RED}${BOLD}$FAIL test(s) failed.${NC}"
    return 1
  fi
}

# =============================================================================
# Native mode (macOS / Windows CI — no Docker)
# =============================================================================

# Finds the host binary to use for native tests.
# Prefers a binary already on PATH (e.g. installed forge), then falls back
# to the debug build in the cargo target directory.
find_native_binary() {
  if command -v forge > /dev/null 2>&1; then
    echo "forge"
    return 0
  fi

  # Try arch-specific debug build
  local arch
  arch=$(uname -m 2>/dev/null || echo "x86_64")
  local candidates=()

  case "$arch" in
    arm64|aarch64)
      candidates+=(
        "$PROJECT_ROOT/target/aarch64-apple-darwin/debug/forge"
        "$PROJECT_ROOT/target/aarch64-unknown-linux-musl/debug/forge"
        "$PROJECT_ROOT/target/aarch64-unknown-linux-gnu/debug/forge"
      )
      ;;
    *)
      candidates+=(
        "$PROJECT_ROOT/target/x86_64-unknown-linux-musl/debug/forge"
        "$PROJECT_ROOT/target/x86_64-unknown-linux-gnu/debug/forge"
        "$PROJECT_ROOT/target/x86_64-apple-darwin/debug/forge"
        "$PROJECT_ROOT/target/x86_64-pc-windows-msvc/debug/forge.exe"
      )
      ;;
  esac

  # Also try the plain debug build (no cross-compilation target)
  candidates+=("$PROJECT_ROOT/target/debug/forge" "$PROJECT_ROOT/target/debug/forge.exe")

  for candidate in "${candidates[@]}"; do
    if [ -f "$candidate" ]; then
      echo "$candidate"
      return 0
    fi
  done

  echo ""
  return 1
}

# Builds the host binary for native mode.
build_native_binary() {
  if [ "$SKIP_BUILD" = true ]; then
    local bin
    bin=$(find_native_binary)
    if [ -n "$bin" ]; then
      log_info "Skipping build, using existing binary: $bin"
      return 0
    fi
    log_fail "SKIP_BUILD=true but no binary found"
    return 1
  fi

  log_info "Building host binary with cargo (debug)..."
  if ! cargo build 2>"$RESULTS_DIR/build-native.log"; then
    log_fail "Native build failed"
    cat "$RESULTS_DIR/build-native.log" >&2
    return 1
  fi
  log_pass "Built host binary -> $(du -h "$PROJECT_ROOT/target/debug/forge" 2>/dev/null | cut -f1 || echo "ok")"
}

# Runs the verify script directly on the host (no Docker).
# Used for macOS and Windows CI runners.
run_native_tests() {
  log_header "Phase 2: Native Host Tests"

  local forge_bin
  forge_bin=$(find_native_binary)
  if [ -z "$forge_bin" ]; then
    log_fail "No forge binary found for native tests. Run without --skip-build or build first."
    return 1
  fi

  log_info "Using binary: $forge_bin"

  # Write the verify script to a temp file
  local verify_script
  verify_script=$(mktemp "$RESULTS_DIR/verify_native_XXXXXX.sh")
  generate_verify_script > "$verify_script"
  chmod +x "$verify_script"

  # Run on host, with forge on PATH
  local dir_with_forge
  dir_with_forge=$(dirname "$forge_bin")

  local raw_output
  raw_output=$(PATH="$dir_with_forge:$PATH" bash "$verify_script" 2>&1) || true

  rm -f "$verify_script"

  # Evaluate CHECK lines
  local eval_result
  eval_result=$(parse_check_lines "$raw_output")
  local status
  local details
  status=$(head -1 <<< "$eval_result")
  details=$(tail -n +2 <<< "$eval_result")

  # Write result file so collect_results picks it up
  local os_label
  os_label=$(uname -s 2>/dev/null || echo "host")
  cat > "$RESULTS_DIR/native-${os_label}.result" <<EOF
STATUS: ${status}
LABEL: ${os_label} (native)
TARGET: host
DETAILS: ${details}
EOF
  echo "$raw_output" > "$RESULTS_DIR/native-${os_label}.output"
}

# =============================================================================
# Main
# =============================================================================

main() {
  parse_args "$@"

  RESULTS_DIR=$(mktemp -d)
  trap 'rm -rf "$RESULTS_DIR"' EXIT

  echo ""
  echo -e "${BOLD}${BLUE}Shell-Native Provider Auth E2E Tests${NC}"
  echo -e "${DIM}Host arch: $HOST_ARCH${NC}"
  echo -e "${DIM}Mode: $MODE${NC}"
  echo ""

  if [ "$MODE" = "quick" ]; then
    run_static_checks
    print_summary
    return
  fi

  if [ "$MODE" = "native" ]; then
    # Phase 1: Build host binary
    log_header "Phase 1: Build Binary"
    build_native_binary

    # Phase 2: Run tests directly on host
    run_native_tests

    # Phase 3: Collect and display results
    collect_results

    print_summary
    return
  fi

  # Phase 1: Build cross-compiled binaries (Linux Docker mode)
  build_all_targets

  # Phase 2: Run Docker tests in parallel
  log_header "Phase 2: Docker Tests"

  if ! command -v docker > /dev/null 2>&1; then
    log_skip "Docker not available — skipping container tests"
  else
    launch_parallel_tests
  fi

  # Phase 3: Collect and display results
  collect_results

  print_summary
}

main "$@"
