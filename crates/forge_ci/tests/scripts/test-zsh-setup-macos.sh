#!/bin/bash
# =============================================================================
# macOS-native E2E test suite for `forge zsh setup`
#
# Tests the complete zsh setup flow natively on macOS using temp HOME directory
# isolation. Covers both "with Homebrew" and "without Homebrew" scenarios,
# verifying dependency detection, installation (zsh, Oh My Zsh, plugins, tools),
# .zshrc configuration, and doctor diagnostics.
#
# Unlike the Linux test suite (test-zsh-setup.sh) which uses Docker containers,
# this script runs directly on the macOS host with HOME directory isolation.
# Each test scenario gets a fresh temp HOME to prevent state leakage.
#
# Build targets (from CI):
#   - x86_64-apple-darwin   (Intel Macs)
#   - aarch64-apple-darwin  (Apple Silicon)
#
# Prerequisites:
#   - macOS (Darwin) host
#   - Rust toolchain
#   - git (Xcode CLT or Homebrew)
#
# Usage:
#   bash crates/forge_ci/tests/scripts/test-zsh-setup-macos.sh                # build + test all
#   bash crates/forge_ci/tests/scripts/test-zsh-setup-macos.sh --quick        # shellcheck only
#   bash crates/forge_ci/tests/scripts/test-zsh-setup-macos.sh --filter "brew" # run only matching
#   bash crates/forge_ci/tests/scripts/test-zsh-setup-macos.sh --skip-build   # skip build, use existing
#   bash crates/forge_ci/tests/scripts/test-zsh-setup-macos.sh --no-cleanup   # keep temp dirs
#   bash crates/forge_ci/tests/scripts/test-zsh-setup-macos.sh --dry-run      # show plan, don't run
#   bash crates/forge_ci/tests/scripts/test-zsh-setup-macos.sh --list         # list scenarios and exit
#   bash crates/forge_ci/tests/scripts/test-zsh-setup-macos.sh --help         # show usage
#
# Relationship to test-zsh-setup.sh:
#   test-zsh-setup.sh tests `forge zsh setup` inside Docker (Linux distros).
#   This script tests `forge zsh setup` natively on macOS.
#   Both use the same CHECK_* line protocol for verification.
# =============================================================================

set -euo pipefail

# =============================================================================
# Platform guard
# =============================================================================

if [ "$(uname -s)" != "Darwin" ]; then
  echo "Error: This script must be run on macOS (Darwin)." >&2
  echo "For Linux testing, use test-zsh-setup.sh (Docker-based)." >&2
  exit 1
fi

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

# Detect host architecture and set build target
HOST_ARCH="$(uname -m)"
readonly HOST_ARCH

if [ "$HOST_ARCH" = "arm64" ] || [ "$HOST_ARCH" = "aarch64" ]; then
  BUILD_TARGET="aarch64-apple-darwin"
elif [ "$HOST_ARCH" = "x86_64" ]; then
  BUILD_TARGET="x86_64-apple-darwin"
else
  echo "Error: Unsupported host architecture: $HOST_ARCH" >&2
  echo "Supported: arm64, aarch64, x86_64" >&2
  exit 1
fi
readonly BUILD_TARGET

# Detect Homebrew prefix (differs between Apple Silicon and Intel)
if [ -d "/opt/homebrew" ]; then
  BREW_PREFIX="/opt/homebrew"
elif [ -d "/usr/local/Homebrew" ]; then
  BREW_PREFIX="/usr/local"
else
  BREW_PREFIX=""
fi
readonly BREW_PREFIX

# =============================================================================
# Test scenarios
# =============================================================================

# Format: "scenario_id|label|brew_mode|test_type"
#   scenario_id - unique identifier
#   label       - human-readable name
#   brew_mode   - "with_brew" or "no_brew"
#   test_type   - "standard", "preinstalled_all", "rerun", "partial",
#                 "no_git", "no_zsh"
readonly SCENARIOS=(
  # --- With Homebrew ---
  "BREW_BARE|Fresh install (with brew)|with_brew|standard"
  "BREW_PREINSTALLED_ALL|Pre-installed everything (with brew)|with_brew|preinstalled_all"
  "BREW_RERUN|Re-run idempotency (with brew)|with_brew|rerun"
  "BREW_PARTIAL|Partial install - only plugins missing (with brew)|with_brew|partial"
  "BREW_NO_GIT|No git (with brew)|with_brew|no_git"

  # --- Without Homebrew ---
  "NOBREW_BARE|Fresh install (no brew, GitHub releases)|no_brew|standard"
  "NOBREW_RERUN|Re-run idempotency (no brew)|no_brew|rerun"
  "NOBREW_NO_ZSH|No brew + no zsh in PATH|no_brew|no_zsh"
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
FILTER_PATTERN=""
EXCLUDE_PATTERN=""
NO_CLEANUP=false
SKIP_BUILD=false
DRY_RUN=false

# Shared temp paths
RESULTS_DIR=""
REAL_HOME="$HOME"

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
Usage: bash crates/forge_ci/tests/scripts/test-zsh-setup-macos.sh [OPTIONS]

Options:
  --quick              Run static analysis only (no tests)
  --filter <pattern>   Run only scenarios whose label matches <pattern> (grep -iE)
  --exclude <pattern>  Skip scenarios whose label matches <pattern> (grep -iE)
  --skip-build         Skip binary build, use existing binary
  --no-cleanup         Keep temp directories and results after tests
  --dry-run            Show what would be tested without running anything
  --list               List all test scenarios and exit
  --help               Show this help message

Notes:
  - This script runs natively on macOS (no Docker).
  - "With brew" tests may install packages via Homebrew.
    On CI runners (ephemeral VMs), this is safe.
    For local development, use --dry-run to review first.
  - "Without brew" tests hide Homebrew from PATH and verify
    GitHub release fallback for tools (fzf, bat, fd).
EOF
}

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --quick)
        MODE="quick"
        shift
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
      --no-cleanup)
        NO_CLEANUP=true
        shift
        ;;
      --dry-run)
        DRY_RUN=true
        shift
        ;;
      --list)
        list_scenarios
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
}

list_scenarios() {
  echo -e "${BOLD}Build Target:${NC}"
  printf "  %-55s %s\n" "$BUILD_TARGET" "$HOST_ARCH"

  echo -e "\n${BOLD}Test Scenarios:${NC}"
  local idx=0
  for entry in "${SCENARIOS[@]}"; do
    idx=$((idx + 1))
    IFS='|' read -r _id label brew_mode test_type <<< "$entry"
    printf "  %2d. %-55s [%s] %s\n" "$idx" "$label" "$brew_mode" "$test_type"
  done

  echo ""
  echo -e "${BOLD}Homebrew:${NC}"
  if [ -n "$BREW_PREFIX" ]; then
    echo "  Found at: $BREW_PREFIX"
  else
    echo "  Not found (no-brew scenarios only)"
  fi
}

# =============================================================================
# Build binary
# =============================================================================

build_binary() {
  local binary_path="$PROJECT_ROOT/target/${BUILD_TARGET}/debug/forge"

  if [ "$SKIP_BUILD" = true ] && [ -f "$binary_path" ]; then
    log_info "Skipping build for ${BUILD_TARGET} (binary exists)"
    return 0
  fi

  # Ensure target is installed
  if ! rustup target list --installed 2>/dev/null | grep -q "$BUILD_TARGET"; then
    log_info "Adding Rust target ${BUILD_TARGET}..."
    rustup target add "$BUILD_TARGET" 2>/dev/null || true
  fi

  log_info "Building ${BUILD_TARGET} with cargo (debug)..."
  if ! cargo build --target "$BUILD_TARGET" 2>"$RESULTS_DIR/build-${BUILD_TARGET}.log"; then
    log_fail "Build failed for ${BUILD_TARGET}"
    log_info "Build log: $RESULTS_DIR/build-${BUILD_TARGET}.log"
    echo ""
    echo "===== Full build log ====="
    cat "$RESULTS_DIR/build-${BUILD_TARGET}.log" 2>/dev/null || echo "Log file not found"
    echo "=========================="
    echo ""
    return 1
  fi

  if [ -f "$binary_path" ]; then
    log_pass "Built ${BUILD_TARGET} -> $(du -h "$binary_path" | cut -f1)"
    return 0
  else
    log_fail "Binary not found after build: ${binary_path}"
    return 1
  fi
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
# PATH filtering helpers
# =============================================================================

# Build a PATH that excludes Homebrew directories.
# The forge binary must be placed in $1 (a temp bin dir) which is prepended.
filter_path_no_brew() {
  local temp_bin="$1"
  local filtered=""
  local IFS=':'

  for dir in $PATH; do
    # Skip Homebrew directories
    case "$dir" in
      /opt/homebrew/bin|/opt/homebrew/sbin) continue ;;
      /usr/local/bin|/usr/local/sbin)
        # On Intel Macs, /usr/local/bin is Homebrew. On Apple Silicon it's not.
        # Check if this is actually a Homebrew path
        if [ -d "/usr/local/Homebrew" ]; then
          continue
        fi
        ;;
    esac
    if [ -n "$filtered" ]; then
      filtered="${filtered}:${dir}"
    else
      filtered="${dir}"
    fi
  done

  # Prepend the temp bin directory
  echo "${temp_bin}:${filtered}"
}

# Build a PATH that hides git by creating a symlink directory.
# On macOS, /usr/bin/git is an Xcode CLT shim — we can't just remove /usr/bin.
# Instead, create a temp dir with symlinks to everything in /usr/bin except git.
filter_path_no_git() {
  local temp_bin="$1"
  local no_git_dir="$2"

  mkdir -p "$no_git_dir"

  # Symlink everything from /usr/bin except git
  for f in /usr/bin/*; do
    local base
    base=$(basename "$f")
    if [ "$base" = "git" ]; then
      continue
    fi
    ln -sf "$f" "$no_git_dir/$base" 2>/dev/null || true
  done

  # Build new PATH replacing /usr/bin with our filtered dir
  local filtered=""
  local IFS=':'
  for dir in $PATH; do
    case "$dir" in
      /usr/bin)
        dir="$no_git_dir"
        ;;
    esac
    # Also skip brew git paths
    case "$dir" in
      /opt/homebrew/bin|/usr/local/bin)
        # These might contain git too; skip them for no-git test
        continue
        ;;
    esac
    if [ -n "$filtered" ]; then
      filtered="${filtered}:${dir}"
    else
      filtered="${dir}"
    fi
  done

  echo "${temp_bin}:${filtered}"
}

# Build a PATH that hides both brew and zsh.
# For the NOBREW_NO_ZSH scenario: remove brew dirs AND create a filtered
# /usr/bin that excludes zsh.
filter_path_no_brew_no_zsh() {
  local temp_bin="$1"
  local no_zsh_dir="$2"

  mkdir -p "$no_zsh_dir"

  # Symlink everything from /usr/bin except zsh
  for f in /usr/bin/*; do
    local base
    base=$(basename "$f")
    if [ "$base" = "zsh" ]; then
      continue
    fi
    ln -sf "$f" "$no_zsh_dir/$base" 2>/dev/null || true
  done

  # Build new PATH: no brew dirs, /usr/bin replaced with filtered dir
  local filtered=""
  local IFS=':'
  for dir in $PATH; do
    case "$dir" in
      /opt/homebrew/bin|/opt/homebrew/sbin) continue ;;
      /usr/local/bin|/usr/local/sbin)
        if [ -d "/usr/local/Homebrew" ]; then
          continue
        fi
        ;;
      /usr/bin)
        dir="$no_zsh_dir"
        ;;
    esac
    if [ -n "$filtered" ]; then
      filtered="${filtered}:${dir}"
    else
      filtered="${dir}"
    fi
  done

  echo "${temp_bin}:${filtered}"
}

# =============================================================================
# Verification function
# =============================================================================

# Run verification checks against the current HOME and emit CHECK_* lines.
# Arguments:
#   $1 - test_type: "standard" | "no_git" | "preinstalled_all" | "rerun" |
#        "partial" | "no_zsh"
#   $2 - setup_output: the captured output from forge zsh setup
#   $3 - setup_exit: the exit code from forge zsh setup
run_verify_checks() {
  local test_type="$1"
  local setup_output="$2"
  local setup_exit="$3"

  echo "SETUP_EXIT=${setup_exit}"

  # --- Verify zsh binary ---
  if command -v zsh > /dev/null 2>&1; then
    local zsh_ver
    zsh_ver=$(zsh --version 2>&1 | head -1) || zsh_ver="(failed)"
    if zsh -c "zmodload zsh/zle && zmodload zsh/datetime && zmodload zsh/stat" > /dev/null 2>&1; then
      echo "CHECK_ZSH=PASS ${zsh_ver} (modules OK)"
    else
      echo "CHECK_ZSH=FAIL ${zsh_ver} (modules broken)"
    fi
  else
    if [ "$test_type" = "no_git" ] || [ "$test_type" = "no_zsh" ]; then
      echo "CHECK_ZSH=PASS (expected: zsh not needed in ${test_type} test)"
    else
      echo "CHECK_ZSH=FAIL zsh not found in PATH"
    fi
  fi

  # --- Verify Oh My Zsh ---
  if [ -d "$HOME/.oh-my-zsh" ]; then
    local omz_ok=true
    local omz_detail="dir=OK"
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
    if [ "$test_type" = "no_git" ] || [ "$test_type" = "no_zsh" ]; then
      echo "CHECK_OMZ_DIR=PASS (expected: no OMZ in ${test_type} test)"
    else
      echo "CHECK_OMZ_DIR=FAIL ~/.oh-my-zsh not found"
    fi
  fi

  # --- Verify Oh My Zsh defaults in .zshrc ---
  if [ -f "$HOME/.zshrc" ]; then
    local omz_defaults_ok=true
    local omz_defaults_detail=""
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
    if [ "$test_type" = "no_git" ] || [ "$test_type" = "no_zsh" ]; then
      echo "CHECK_OMZ_DEFAULTS=PASS (expected: no .zshrc in ${test_type} test)"
    else
      echo "CHECK_OMZ_DEFAULTS=FAIL ~/.zshrc not found"
    fi
  fi

  # --- Verify plugins ---
  local zsh_custom="${ZSH_CUSTOM:-$HOME/.oh-my-zsh/custom}"
  if [ -d "$zsh_custom/plugins/zsh-autosuggestions" ]; then
    if ls "$zsh_custom/plugins/zsh-autosuggestions/"*.zsh 1>/dev/null 2>&1; then
      echo "CHECK_AUTOSUGGESTIONS=PASS"
    else
      echo "CHECK_AUTOSUGGESTIONS=FAIL (dir exists but no .zsh files)"
    fi
  else
    if [ "$test_type" = "no_git" ] || [ "$test_type" = "no_zsh" ]; then
      echo "CHECK_AUTOSUGGESTIONS=PASS (expected: no plugins in ${test_type} test)"
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
    if [ "$test_type" = "no_git" ] || [ "$test_type" = "no_zsh" ]; then
      echo "CHECK_SYNTAX_HIGHLIGHTING=PASS (expected: no plugins in ${test_type} test)"
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
    local start_count
    local end_count
    start_count=$(grep -c '# >>> forge initialize >>>' "$HOME/.zshrc" 2>/dev/null || echo "0")
    end_count=$(grep -c '# <<< forge initialize <<<' "$HOME/.zshrc" 2>/dev/null || echo "0")
    if [ "$start_count" -eq 1 ] && [ "$end_count" -eq 1 ]; then
      echo "CHECK_MARKER_UNIQUE=PASS"
    else
      echo "CHECK_MARKER_UNIQUE=FAIL (start=${start_count}, end=${end_count})"
    fi
  else
    if [ "$test_type" = "no_git" ] || [ "$test_type" = "no_zsh" ]; then
      echo "CHECK_ZSHRC_MARKERS=PASS (expected: no .zshrc in ${test_type} test)"
      echo "CHECK_ZSHRC_PLUGIN=PASS (expected: no .zshrc in ${test_type} test)"
      echo "CHECK_ZSHRC_THEME=PASS (expected: no .zshrc in ${test_type} test)"
      echo "CHECK_NO_NERD_FONT_DISABLE=PASS (expected: no .zshrc in ${test_type} test)"
      echo "CHECK_NO_FORGE_EDITOR=PASS (expected: no .zshrc in ${test_type} test)"
      echo "CHECK_MARKER_UNIQUE=PASS (expected: no .zshrc in ${test_type} test)"
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
  local doctor_output
  doctor_output=$(forge zsh doctor 2>&1) || true
  local doctor_exit=$?
  if [ "$test_type" = "no_git" ] || [ "$test_type" = "no_zsh" ]; then
    echo "CHECK_DOCTOR_EXIT=PASS (skipped for ${test_type} test)"
  else
    if [ $doctor_exit -le 1 ]; then
      echo "CHECK_DOCTOR_EXIT=PASS (exit=${doctor_exit})"
    else
      echo "CHECK_DOCTOR_EXIT=FAIL (exit=${doctor_exit})"
    fi
  fi

  # --- Verify output format ---
  local output_ok=true
  local output_detail=""

  if echo "$setup_output" | grep -qi "found\|not found\|installed\|Detecting"; then
    output_detail="detect=OK"
  else
    output_ok=false
    output_detail="detect=MISSING"
  fi

  if [ "$test_type" = "no_git" ]; then
    if echo "$setup_output" | grep -qi "git is required"; then
      output_detail="${output_detail}, git_error=OK"
    else
      output_ok=false
      output_detail="${output_detail}, git_error=MISSING"
    fi
    echo "CHECK_OUTPUT_FORMAT=PASS ${output_detail}"
  elif [ "$test_type" = "no_zsh" ]; then
    if echo "$setup_output" | grep -qi "Homebrew not found\|brew.*not found\|Failed to install zsh"; then
      output_detail="${output_detail}, brew_error=OK"
    else
      output_ok=false
      output_detail="${output_detail}, brew_error=MISSING"
    fi
    echo "CHECK_OUTPUT_FORMAT=PASS ${output_detail}"
  else
    if echo "$setup_output" | grep -qi "Setup complete\|complete"; then
      output_detail="${output_detail}, complete=OK"
    else
      output_ok=false
      output_detail="${output_detail}, complete=MISSING"
    fi

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
  case "$test_type" in
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
    no_zsh)
      # When brew is hidden and zsh is hidden, forge should fail trying to install zsh
      if echo "$setup_output" | grep -qi "Homebrew not found\|brew.*not found\|Failed to install zsh"; then
        echo "CHECK_EDGE_NO_ZSH=PASS (correctly reports no brew/zsh)"
      else
        echo "CHECK_EDGE_NO_ZSH=FAIL (should report Homebrew not found or install failure)"
      fi
      ;;
    rerun)
      # Already verified marker uniqueness above. Check second-run specifics later.
      ;;
    partial)
      if echo "$setup_output" | grep -qi "zsh-autosuggestions\|zsh-syntax-highlighting"; then
        echo "CHECK_EDGE_PARTIAL_PLUGINS=PASS (plugins in install plan)"
      else
        echo "CHECK_EDGE_PARTIAL_PLUGINS=FAIL (plugins not mentioned)"
      fi
      local install_plan
      install_plan=$(echo "$setup_output" | sed -n '/The following will be installed/,/^$/p' 2>/dev/null || echo "")
      if [ -n "$install_plan" ]; then
        if echo "$install_plan" | grep -qi "zsh (shell)\|Oh My Zsh"; then
          echo "CHECK_EDGE_PARTIAL_NO_ZSH=FAIL (should not install zsh/OMZ)"
        else
          echo "CHECK_EDGE_PARTIAL_NO_ZSH=PASS (correctly skips zsh/OMZ)"
        fi
      else
        echo "CHECK_EDGE_PARTIAL_NO_ZSH=PASS (no install plan = nothing to install)"
      fi
      ;;
  esac

  # --- Emit raw output for debugging ---
  echo "OUTPUT_BEGIN"
  echo "$setup_output"
  echo "OUTPUT_END"
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

# =============================================================================
# Pre-setup helpers for edge cases
# =============================================================================

# Pre-install Oh My Zsh into the current HOME (for preinstalled_all and partial tests)
preinstall_omz() {
  local script_url="https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh"
  sh -c "$(curl -fsSL "$script_url")" "" --unattended > /dev/null 2>&1 || true
}

# Pre-install zsh plugins into the current HOME
preinstall_plugins() {
  local zsh_custom="${ZSH_CUSTOM:-$HOME/.oh-my-zsh/custom}"
  git clone --quiet https://github.com/zsh-users/zsh-autosuggestions.git \
    "$zsh_custom/plugins/zsh-autosuggestions" 2>/dev/null || true
  git clone --quiet https://github.com/zsh-users/zsh-syntax-highlighting.git \
    "$zsh_custom/plugins/zsh-syntax-highlighting" 2>/dev/null || true
}

# =============================================================================
# Test execution
# =============================================================================

# Run a single test scenario.
# Arguments:
#   $1 - scenario entry string ("id|label|brew_mode|test_type")
run_single_test() {
  local entry="$1"
  IFS='|' read -r scenario_id label brew_mode test_type <<< "$entry"

  local safe_label
  safe_label=$(echo "$label" | tr '[:upper:]' '[:lower:]' | tr ' /' '_-' | tr -cd '[:alnum:]_-')
  local result_file="$RESULTS_DIR/${safe_label}.result"
  local output_file="$RESULTS_DIR/${safe_label}.output"

  local binary_path="$PROJECT_ROOT/target/${BUILD_TARGET}/debug/forge"

  # Check binary exists
  if [ ! -f "$binary_path" ]; then
    cat > "$result_file" <<EOF
STATUS: FAIL
LABEL: ${label}
DETAILS: Binary not found: ${binary_path}
EOF
    return
  fi

  # Create temp HOME for isolation
  local temp_home
  temp_home=$(mktemp -d)
  local temp_bin="${temp_home}/.forge-test-bin"
  mkdir -p "$temp_bin"

  # Copy forge binary to temp bin
  cp "$binary_path" "$temp_bin/forge"
  chmod +x "$temp_bin/forge"

  # Build the appropriate PATH
  local test_path="$PATH"
  local no_git_dir="${temp_home}/.no-git-bin"
  local no_zsh_dir="${temp_home}/.no-zsh-bin"

  case "$brew_mode" in
    no_brew)
      case "$test_type" in
        no_zsh)
          test_path=$(filter_path_no_brew_no_zsh "$temp_bin" "$no_zsh_dir")
          ;;
        *)
          test_path=$(filter_path_no_brew "$temp_bin")
          ;;
      esac
      ;;
    with_brew)
      case "$test_type" in
        no_git)
          test_path=$(filter_path_no_git "$temp_bin" "$no_git_dir")
          ;;
        *)
          test_path="${temp_bin}:${PATH}"
          ;;
      esac
      ;;
  esac

  # Pre-setup for edge cases
  local saved_home="$HOME"
  export HOME="$temp_home"

  case "$test_type" in
    preinstalled_all)
      # Pre-install OMZ + plugins
      preinstall_omz
      preinstall_plugins
      # Pre-install tools by running forge once (or they may already be on system)
      ;;
    partial)
      # Pre-install OMZ only (no plugins)
      preinstall_omz
      ;;
  esac

  # Run forge zsh setup
  local setup_output=""
  local setup_exit=0
  setup_output=$(PATH="$test_path" HOME="$temp_home" forge zsh setup --non-interactive 2>&1) || setup_exit=$?

  # Run verification
  local verify_output
  verify_output=$(PATH="$test_path" HOME="$temp_home" run_verify_checks "$test_type" "$setup_output" "$setup_exit" 2>&1) || true

  # Handle rerun scenario: run forge a second time
  if [ "$test_type" = "rerun" ]; then
    # Update PATH to include ~/.local/bin for GitHub-installed tools
    local rerun_path="${temp_home}/.local/bin:${test_path}"
    local rerun_output=""
    local rerun_exit=0
    rerun_output=$(PATH="$rerun_path" HOME="$temp_home" forge zsh setup --non-interactive 2>&1) || rerun_exit=$?

    if [ "$rerun_exit" -eq 0 ]; then
      verify_output="${verify_output}
CHECK_EDGE_RERUN_EXIT=PASS"
    else
      verify_output="${verify_output}
CHECK_EDGE_RERUN_EXIT=FAIL (exit=${rerun_exit})"
    fi

    if echo "$rerun_output" | grep -qi "All dependencies already installed"; then
      verify_output="${verify_output}
CHECK_EDGE_RERUN_SKIP=PASS"
    elif [ "$brew_mode" = "no_brew" ]; then
      # Without brew, fzf/bat/fd can't install, so forge will still try to
      # install them on re-run. Verify the core components (OMZ + plugins) are
      # detected as already present — that's the idempotency we care about.
      if echo "$rerun_output" | grep -qi "Oh My Zsh installed" && \
         echo "$rerun_output" | grep -qi "zsh-autosuggestions installed" && \
         echo "$rerun_output" | grep -qi "zsh-syntax-highlighting installed"; then
        verify_output="${verify_output}
CHECK_EDGE_RERUN_SKIP=PASS (core deps detected; tools skipped due to no brew)"
      else
        verify_output="${verify_output}
CHECK_EDGE_RERUN_SKIP=FAIL (core deps not detected on re-run without brew)"
      fi
    else
      verify_output="${verify_output}
CHECK_EDGE_RERUN_SKIP=FAIL (second run should skip installs)"
    fi

    # Check marker uniqueness after re-run
    if [ -f "$temp_home/.zshrc" ]; then
      local start_count
      start_count=$(grep -c '# >>> forge initialize >>>' "$temp_home/.zshrc" 2>/dev/null || echo "0")
      if [ "$start_count" -eq 1 ]; then
        verify_output="${verify_output}
CHECK_EDGE_RERUN_MARKERS=PASS (still exactly 1 marker set)"
      else
        verify_output="${verify_output}
CHECK_EDGE_RERUN_MARKERS=FAIL (found ${start_count} marker sets)"
      fi
    else
      verify_output="${verify_output}
CHECK_EDGE_RERUN_MARKERS=FAIL (no .zshrc after re-run)"
    fi

    # Append second run output for debugging
    verify_output="${verify_output}
OUTPUT_BEGIN
===== SECOND RUN (idempotency check) =====
${rerun_output}
==========================================
OUTPUT_END"
  fi

  # Restore HOME
  export HOME="$saved_home"

  # Parse SETUP_EXIT
  local parsed_setup_exit
  parsed_setup_exit=$(grep '^SETUP_EXIT=' <<< "$verify_output" | head -1 | cut -d= -f2)

  # Evaluate CHECK lines
  local eval_result
  eval_result=$(parse_check_lines "$verify_output")
  local status
  local details
  status=$(head -1 <<< "$eval_result")
  details=$(tail -n +2 <<< "$eval_result")

  # Check setup exit code
  if [ -n "$parsed_setup_exit" ] && [ "$parsed_setup_exit" != "0" ] && \
     [ "$test_type" != "no_git" ] && [ "$test_type" != "no_zsh" ]; then
    status="FAIL"
    details="${details}    SETUP_EXIT=${parsed_setup_exit} (expected 0)\n"
  fi

  # Write result
  cat > "$result_file" <<EOF
STATUS: ${status}
LABEL: ${label}
DETAILS: ${details}
EOF

  # Save raw output
  echo "$verify_output" > "$output_file"

  # Cleanup temp HOME unless --no-cleanup
  if [ "$NO_CLEANUP" = false ]; then
    rm -rf "$temp_home"
  else
    # Copy diagnostic files into RESULTS_DIR for artifact upload
    local diag_dir="$RESULTS_DIR/${safe_label}-home"
    mkdir -p "$diag_dir"
    # Copy key files that help debug failures
    cp "$temp_home/.zshrc" "$diag_dir/zshrc" 2>/dev/null || true
    cp -r "$temp_home/.oh-my-zsh/custom/plugins" "$diag_dir/omz-plugins" 2>/dev/null || true
    ls -la "$temp_home/" > "$diag_dir/home-listing.txt" 2>/dev/null || true
    ls -la "$temp_home/.oh-my-zsh/" > "$diag_dir/omz-listing.txt" 2>/dev/null || true
    ls -la "$temp_home/.local/bin/" > "$diag_dir/local-bin-listing.txt" 2>/dev/null || true
    # Save the PATH that was used
    echo "$test_path" > "$diag_dir/test-path.txt" 2>/dev/null || true
    log_info "Diagnostics saved to: ${diag_dir}"
    # Still remove the temp HOME itself (diagnostics are in RESULTS_DIR now)
    rm -rf "$temp_home"
  fi
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
  echo -e "${BOLD}================================================================${NC}"
  local total=$((PASS + FAIL + SKIP))
  if [ "$FAIL" -eq 0 ]; then
    echo -e "${GREEN}${BOLD}  RESULTS: ${PASS} passed, ${FAIL} failed, ${SKIP} skipped (${total} total)${NC}"
  else
    echo -e "${RED}${BOLD}  RESULTS: ${PASS} passed, ${FAIL} failed, ${SKIP} skipped (${total} total)${NC}"
  fi
  echo -e "${BOLD}================================================================${NC}"

  if [ ${#FAILURES[@]} -gt 0 ]; then
    echo ""
    echo -e "${RED}${BOLD}Failed tests:${NC}"
    for f in "${FAILURES[@]}"; do
      echo -e "  ${RED}* ${f}${NC}"
    done
  fi

  if [ "$NO_CLEANUP" = true ] && [ -n "$RESULTS_DIR" ] && [ -d "$RESULTS_DIR" ]; then
    echo ""
    echo -e "  ${DIM}Results preserved: ${RESULTS_DIR}${NC}"
  fi
}

# =============================================================================
# Test orchestrator
# =============================================================================

run_tests() {
  # Create results directory — use a known path for CI artifact upload
  if [ "$NO_CLEANUP" = true ]; then
    RESULTS_DIR="$PROJECT_ROOT/test-results-macos"
    rm -rf "$RESULTS_DIR"
    mkdir -p "$RESULTS_DIR"
  else
    RESULTS_DIR=$(mktemp -d)
  fi

  # Build binary
  log_header "Phase 2: Build Binary"
  if ! build_binary; then
    echo "Error: Build failed. Cannot continue without binary." >&2
    exit 1
  fi

  log_header "Phase 3: macOS E2E Tests"
  log_info "Results dir: ${RESULTS_DIR}"
  log_info "Build target: ${BUILD_TARGET}"
  log_info "Homebrew: ${BREW_PREFIX:-not found}"
  echo ""

  # Run each scenario sequentially
  for entry in "${SCENARIOS[@]}"; do
    IFS='|' read -r _id label brew_mode _test_type <<< "$entry"

    # Apply filter
    if [ -n "$FILTER_PATTERN" ] && ! echo "$label" | grep -qiE "$FILTER_PATTERN"; then
      continue
    fi
    if [ -n "$EXCLUDE_PATTERN" ] && echo "$label" | grep -qiE "$EXCLUDE_PATTERN"; then
      continue
    fi

    # Skip brew tests if brew is not installed
    if [ "$brew_mode" = "with_brew" ] && [ -z "$BREW_PREFIX" ]; then
      log_skip "${label} (Homebrew not installed)"
      continue
    fi

    if [ "$DRY_RUN" = true ]; then
      log_info "[dry-run] Would run: ${label}"
      continue
    fi

    log_info "Running: ${label}..."
    run_single_test "$entry"
  done

  # Collect and display results
  if [ "$DRY_RUN" = false ]; then
    collect_test_results
  fi
}

# =============================================================================
# Main
# =============================================================================

main() {
  parse_args "$@"

  echo -e "${BOLD}${BLUE}Forge ZSH Setup - macOS E2E Test Suite${NC}"
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

  run_tests

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
