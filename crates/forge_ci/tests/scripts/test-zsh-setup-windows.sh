#!/bin/bash
# =============================================================================
# Windows/Git Bash-native E2E test suite for `forge zsh setup`
#
# Tests the complete zsh setup flow natively on Windows using Git Bash with
# temp HOME directory isolation. Covers dependency detection, MSYS2 package
# download + zsh installation, Oh My Zsh + plugin installation, .bashrc
# auto-start configuration (Windows-specific), .zshrc forge marker
# configuration, and doctor diagnostics.
#
# Unlike the Linux test suite (test-zsh-setup.sh) which uses Docker containers,
# and the macOS suite (test-zsh-setup-macos.sh) which runs natively on macOS,
# this script runs directly on Windows inside Git Bash with HOME directory
# isolation. Each test scenario gets a fresh temp HOME to prevent state leakage.
#
# Build targets (auto-detected from architecture):
#   - x86_64-pc-windows-msvc  (x86_64 runners)
#   - aarch64-pc-windows-msvc (ARM64 runners)
#
# Prerequisites:
#   - Windows with Git Bash (Git for Windows)
#   - Rust toolchain
#   - Network access (MSYS2 repo, GitHub for Oh My Zsh + plugins)
#
# Usage:
#   bash crates/forge_ci/tests/scripts/test-zsh-setup-windows.sh                # build + test all
#   bash crates/forge_ci/tests/scripts/test-zsh-setup-windows.sh --quick        # shellcheck only
#   bash crates/forge_ci/tests/scripts/test-zsh-setup-windows.sh --filter "fresh" # run only matching
#   bash crates/forge_ci/tests/scripts/test-zsh-setup-windows.sh --skip-build   # skip build, use existing
#   bash crates/forge_ci/tests/scripts/test-zsh-setup-windows.sh --no-cleanup   # keep temp dirs
#   bash crates/forge_ci/tests/scripts/test-zsh-setup-windows.sh --dry-run      # show plan, don't run
#   bash crates/forge_ci/tests/scripts/test-zsh-setup-windows.sh --list         # list scenarios and exit
#   bash crates/forge_ci/tests/scripts/test-zsh-setup-windows.sh --help         # show usage
#
# Relationship to sibling test suites:
#   test-zsh-setup.sh        — Docker-based E2E tests for Linux distros
#   test-zsh-setup-macos.sh  — Native E2E tests for macOS
#   test-zsh-setup-windows.sh — Native E2E tests for Windows/Git Bash (this file)
#   All three use the same CHECK_* line protocol for verification.
# =============================================================================

set -euo pipefail

# =============================================================================
# Platform guard
# =============================================================================

case "$(uname -s)" in
  MINGW*|MSYS*) ;; # OK — Git Bash / MSYS2
  *)
    echo "Error: This script must be run in Git Bash on Windows." >&2
    echo "For Linux testing, use test-zsh-setup.sh (Docker-based)." >&2
    echo "For macOS testing, use test-zsh-setup-macos.sh." >&2
    exit 1
    ;;
esac

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

# Detect architecture and select build target
case "$(uname -m)" in
  x86_64|AMD64)
    BUILD_TARGET="x86_64-pc-windows-msvc"
    ;;
  aarch64|arm64|ARM64)
    BUILD_TARGET="aarch64-pc-windows-msvc"
    ;;
  *)
    echo "Error: Unsupported architecture: $(uname -m)" >&2
    exit 1
    ;;
esac
readonly BUILD_TARGET

# =============================================================================
# Test scenarios
# =============================================================================

# Format: "scenario_id|label|test_type"
#   scenario_id - unique identifier
#   label       - human-readable name
#   test_type   - "standard", "preinstalled_all", "rerun", "partial"
#
# NOTE: Unlike the Linux/macOS test suites, there is NO "no_git" scenario here.
# On Windows, forge.exe is a native MSVC binary that resolves git through Windows
# PATH resolution (CreateProcessW, where.exe, etc.), not bash PATH. Hiding git
# by filtering the bash PATH or renaming binaries is fundamentally unreliable
# because Git for Windows installs in multiple locations (/usr/bin, /mingw64/bin,
# C:\Program Files\Git\cmd, etc.) and Windows system PATH entries bypass bash.
# The no-git early-exit logic is platform-independent and tested on Linux/macOS.
readonly SCENARIOS=(
  # Standard fresh install — the primary happy path
  "FRESH|Fresh install (Git Bash)|standard"

  # Pre-installed everything — verify fast path (two-pass approach)
  "PREINSTALLED_ALL|Pre-installed everything (fast path)|preinstalled_all"

  # Re-run idempotency — verify no duplicate markers
  "RERUN|Re-run idempotency|rerun"

  # Partial install — only plugins missing
  "PARTIAL|Partial install (only plugins missing)|partial"
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
Usage: bash crates/forge_ci/tests/scripts/test-zsh-setup-windows.sh [OPTIONS]

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
  - This script runs natively in Git Bash on Windows (no Docker).
  - The FRESH scenario downloads MSYS2 packages (zsh, ncurses, etc.) and
    installs zsh into the Git Bash /usr tree. This requires network access
    and may need administrator privileges.
  - Each test scenario uses an isolated temp HOME directory.
  - On CI runners (GitHub Actions windows-latest), administrator access is
    typically available by default.
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
  printf "  %-55s %s\n" "$BUILD_TARGET" "$(uname -m)"

  echo -e "\n${BOLD}Test Scenarios:${NC}"
  local idx=0
  for entry in "${SCENARIOS[@]}"; do
    idx=$((idx + 1))
    IFS='|' read -r _id label test_type <<< "$entry"
    printf "  %2d. %-55s %s\n" "$idx" "$label" "$test_type"
  done
}

# =============================================================================
# Build binary
# =============================================================================

build_binary() {
  local binary_path="$PROJECT_ROOT/target/${BUILD_TARGET}/debug/forge.exe"

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
# Verification function
# =============================================================================

# Run verification checks against the current HOME and emit CHECK_* lines.
# Arguments:
#   $1 - test_type: "standard" | "preinstalled_all" | "rerun" | "partial"
#   $2 - setup_output: the captured output from forge zsh setup
#   $3 - setup_exit: the exit code from forge zsh setup
run_verify_checks() {
  local test_type="$1"
  local setup_output="$2"
  local setup_exit="$3"

  echo "SETUP_EXIT=${setup_exit}"

  # --- Verify zsh binary ---
  if [ -f "/usr/bin/zsh.exe" ] || command -v zsh > /dev/null 2>&1; then
    local zsh_ver
    zsh_ver=$(zsh --version 2>&1 | head -1) || zsh_ver="(failed)"
    if zsh -c "zmodload zsh/zle && zmodload zsh/datetime && zmodload zsh/stat" > /dev/null 2>&1; then
      echo "CHECK_ZSH=PASS ${zsh_ver} (modules OK)"
    else
      echo "CHECK_ZSH=FAIL ${zsh_ver} (modules broken)"
    fi
  else
    echo "CHECK_ZSH=FAIL zsh not found in PATH or /usr/bin/zsh.exe"
  fi

  # --- Verify zsh.exe is in /usr/bin (Windows-specific) ---
  if [ -f "/usr/bin/zsh.exe" ]; then
    echo "CHECK_ZSH_EXE_LOCATION=PASS"
  else
    echo "CHECK_ZSH_EXE_LOCATION=FAIL (/usr/bin/zsh.exe not found)"
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
    echo "CHECK_OMZ_DIR=FAIL ~/.oh-my-zsh not found"
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
    echo "CHECK_OMZ_DEFAULTS=FAIL ~/.zshrc not found"
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
    echo "CHECK_AUTOSUGGESTIONS=FAIL not installed"
  fi

  if [ -d "$zsh_custom/plugins/zsh-syntax-highlighting" ]; then
    if ls "$zsh_custom/plugins/zsh-syntax-highlighting/"*.zsh 1>/dev/null 2>&1; then
      echo "CHECK_SYNTAX_HIGHLIGHTING=PASS"
    else
      echo "CHECK_SYNTAX_HIGHLIGHTING=FAIL (dir exists but no .zsh files)"
    fi
  else
    echo "CHECK_SYNTAX_HIGHLIGHTING=FAIL not installed"
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
    echo "CHECK_ZSHRC_MARKERS=FAIL no .zshrc"
    echo "CHECK_ZSHRC_PLUGIN=FAIL no .zshrc"
    echo "CHECK_ZSHRC_THEME=FAIL no .zshrc"
    echo "CHECK_NO_NERD_FONT_DISABLE=FAIL no .zshrc"
    echo "CHECK_NO_FORGE_EDITOR=FAIL no .zshrc"
    echo "CHECK_MARKER_UNIQUE=FAIL no .zshrc"
  fi

  # --- Windows-specific: Verify .bashrc auto-start configuration ---
  if [ -f "$HOME/.bashrc" ]; then
    if grep -q '# Added by forge zsh setup' "$HOME/.bashrc" && \
       grep -q 'exec.*zsh' "$HOME/.bashrc"; then
      echo "CHECK_BASHRC_AUTOSTART=PASS"
    else
      echo "CHECK_BASHRC_AUTOSTART=FAIL (auto-start block not found in .bashrc)"
    fi

    # Check uniqueness of auto-start block
    local autostart_count
    autostart_count=$(grep -c '# Added by forge zsh setup' "$HOME/.bashrc" 2>/dev/null || echo "0")
    if [ "$autostart_count" -eq 1 ]; then
      echo "CHECK_BASHRC_MARKER_UNIQUE=PASS"
    else
      echo "CHECK_BASHRC_MARKER_UNIQUE=FAIL (found ${autostart_count} auto-start blocks)"
    fi
  else
    echo "CHECK_BASHRC_AUTOSTART=FAIL (.bashrc not found)"
    echo "CHECK_BASHRC_MARKER_UNIQUE=FAIL (.bashrc not found)"
  fi

  # Check suppression files created by forge
  if [ -f "$HOME/.bash_profile" ]; then
    echo "CHECK_BASH_PROFILE_EXISTS=PASS"
  else
    echo "CHECK_BASH_PROFILE_EXISTS=FAIL"
  fi

  if [ -f "$HOME/.bash_login" ]; then
    echo "CHECK_BASH_LOGIN_EXISTS=PASS"
  else
    echo "CHECK_BASH_LOGIN_EXISTS=FAIL"
  fi

  if [ -f "$HOME/.profile" ]; then
    echo "CHECK_PROFILE_EXISTS=PASS"
  else
    echo "CHECK_PROFILE_EXISTS=FAIL"
  fi


  # --- Check if forge zsh setup's own doctor run failed ---
  # forge zsh setup runs doctor internally. Even if our independent doctor call
  # succeeds (different environment), we must detect if setup's doctor failed.
  if echo "$setup_output" | grep -qi "forge zsh doctor failed"; then
    echo "CHECK_SETUP_DOCTOR=FAIL (setup reported doctor failure)"
  else
    echo "CHECK_SETUP_DOCTOR=PASS"
  fi

  # --- Run forge zsh doctor ---
  local doctor_output
  local doctor_exit=0
  doctor_output=$(forge zsh doctor 2>&1) || doctor_exit=$?
  if [ $doctor_exit -eq 0 ]; then
    echo "CHECK_DOCTOR_EXIT=PASS (exit=0)"
  else
    echo "CHECK_DOCTOR_EXIT=FAIL (exit=${doctor_exit})"
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

  # Windows-specific: check for Git Bash summary message.
  # When setup_fully_successful is true, the output contains "Git Bash" and
  # "source ~/.bashrc". When tools (fzf/bat/fd) fail to install (common on
  # Windows CI — "No package manager on Windows"), the warning message
  # "Setup completed with some errors" is shown instead. Accept either.
  if echo "$setup_output" | grep -qi "Git Bash\|source.*bashrc"; then
    output_detail="${output_detail}, gitbash_summary=OK"
    echo "CHECK_SUMMARY_GITBASH=PASS"
  elif echo "$setup_output" | grep -qi "Setup completed with some errors\|completed with some errors"; then
    output_detail="${output_detail}, gitbash_summary=OK(warning)"
    echo "CHECK_SUMMARY_GITBASH=PASS (warning path: tools install failed but setup completed)"
  else
    output_detail="${output_detail}, gitbash_summary=MISSING"
    echo "CHECK_SUMMARY_GITBASH=FAIL (expected Git Bash summary or warning message)"
  fi

  if [ "$output_ok" = true ]; then
    echo "CHECK_OUTPUT_FORMAT=PASS ${output_detail}"
  else
    echo "CHECK_OUTPUT_FORMAT=FAIL ${output_detail}"
  fi

  # --- Edge-case-specific checks ---
  case "$test_type" in
    preinstalled_all)
      # On Windows CI, fzf/bat/fd are never available ("No package manager on
      # Windows"), so "All dependencies already installed" is never shown — forge
      # still lists fzf/bat/fd in the install plan. Accept the case where only
      # tools (not core deps) are listed for installation.
      if echo "$setup_output" | grep -qi "All dependencies already installed"; then
        echo "CHECK_EDGE_ALL_PRESENT=PASS"
      else
        # Check that core deps (zsh, OMZ, plugins) are NOT in the install plan
        # but only tools (fzf, bat, fd) are listed
        local install_section
        install_section=$(echo "$setup_output" | sed -n '/The following will be installed/,/^$/p' 2>/dev/null || echo "")
        if [ -n "$install_section" ]; then
          if echo "$install_section" | grep -qi "zsh (shell)\|Oh My Zsh\|autosuggestions\|syntax-highlighting"; then
            echo "CHECK_EDGE_ALL_PRESENT=FAIL (core deps should not be in install plan)"
          else
            echo "CHECK_EDGE_ALL_PRESENT=PASS (core deps pre-installed; only tools remain)"
          fi
        else
          echo "CHECK_EDGE_ALL_PRESENT=PASS (no install plan shown)"
        fi
      fi
      if echo "$setup_output" | grep -qi "The following will be installed"; then
        # On Windows, this is expected because fzf/bat/fd are always missing.
        # Verify only tools are in the list, not core deps.
        local install_items
        install_items=$(echo "$setup_output" | sed -n '/The following will be installed/,/^$/p' 2>/dev/null || echo "")
        if echo "$install_items" | grep -qi "zsh (shell)\|Oh My Zsh\|autosuggestions\|syntax-highlighting"; then
          echo "CHECK_EDGE_NO_INSTALL=FAIL (core deps should not be reinstalled)"
        else
          echo "CHECK_EDGE_NO_INSTALL=PASS (only tools listed — core deps correctly skipped)"
        fi
      else
        echo "CHECK_EDGE_NO_INSTALL=PASS (correctly skipped installation)"
      fi
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
# Test execution
# =============================================================================

# Run a single test scenario.
# Arguments:
#   $1 - scenario entry string ("id|label|test_type")
run_single_test() {
  local entry="$1"
  IFS='|' read -r scenario_id label test_type <<< "$entry"

  local safe_label
  safe_label=$(echo "$label" | tr '[:upper:]' '[:lower:]' | tr ' /' '_-' | tr -cd '[:alnum:]_-')
  local result_file="$RESULTS_DIR/${safe_label}.result"
  local output_file="$RESULTS_DIR/${safe_label}.output"

  local binary_path="$PROJECT_ROOT/target/${BUILD_TARGET}/debug/forge.exe"

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
  cp "$binary_path" "$temp_bin/forge.exe"
  chmod +x "$temp_bin/forge.exe"

  # Build the PATH with forge binary prepended
  local test_path="${temp_bin}:${PATH}"

  # Pre-setup for edge cases
  local saved_home="$HOME"
  export HOME="$temp_home"

  case "$test_type" in
    preinstalled_all)
      # Two-pass approach: run forge once as the "pre-install", then test
      # the second run for the fast path detection.
      # First pass — do the full install:
      PATH="$test_path" HOME="$temp_home" NO_COLOR=1 FORGE_EDITOR=vi forge.exe zsh setup --non-interactive > /dev/null 2>&1 || true
      ;;
    partial)
      # Run forge once to get a full install, then remove plugins
      PATH="$test_path" HOME="$temp_home" NO_COLOR=1 FORGE_EDITOR=vi forge.exe zsh setup --non-interactive > /dev/null 2>&1 || true
      # Remove plugins to simulate partial install
      local zsh_custom_dir="${temp_home}/.oh-my-zsh/custom/plugins"
      rm -rf "${zsh_custom_dir}/zsh-autosuggestions" 2>/dev/null || true
      rm -rf "${zsh_custom_dir}/zsh-syntax-highlighting" 2>/dev/null || true
      ;;
  esac

  # Run forge zsh setup
  local setup_output=""
  local setup_exit=0
  setup_output=$(PATH="$test_path" HOME="$temp_home" NO_COLOR=1 FORGE_EDITOR=vi forge.exe zsh setup --non-interactive 2>&1) || setup_exit=$?

  # Strip ANSI escape codes for reliable grep matching
  setup_output=$(printf '%s' "$setup_output" | sed 's/\x1b\[[0-9;]*m//g')

  # Run verification
  local verify_output
  verify_output=$(PATH="$test_path" HOME="$temp_home" FORGE_EDITOR=vi run_verify_checks "$test_type" "$setup_output" "$setup_exit" 2>&1) || true

  # Handle rerun scenario: run forge a second time
  if [ "$test_type" = "rerun" ]; then
    # Update PATH to include ~/.local/bin for GitHub-installed tools
    local rerun_path="${temp_home}/.local/bin:${test_path}"
    local rerun_output=""
    local rerun_exit=0
    rerun_output=$(PATH="$rerun_path" HOME="$temp_home" NO_COLOR=1 FORGE_EDITOR=vi forge.exe zsh setup --non-interactive 2>&1) || rerun_exit=$?
    rerun_output=$(printf '%s' "$rerun_output" | sed 's/\x1b\[[0-9;]*m//g')

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
    else
      # On Windows, fzf/bat/fd are never installable, so "All dependencies
      # already installed" never appears. Instead, check that core deps
      # (zsh, OMZ, plugins) are not in the install plan on the second run.
      local rerun_install_section
      rerun_install_section=$(echo "$rerun_output" | sed -n '/The following will be installed/,/^$/p' 2>/dev/null || echo "")
      if [ -n "$rerun_install_section" ]; then
        if echo "$rerun_install_section" | grep -qi "zsh (shell)\|Oh My Zsh\|autosuggestions\|syntax-highlighting"; then
          verify_output="${verify_output}
CHECK_EDGE_RERUN_SKIP=FAIL (core deps should not be reinstalled on re-run)"
        else
          verify_output="${verify_output}
CHECK_EDGE_RERUN_SKIP=PASS (core deps skipped on re-run; only tools remain)"
        fi
      else
        verify_output="${verify_output}
CHECK_EDGE_RERUN_SKIP=PASS (no install plan on re-run)"
      fi
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

    # Check bashrc auto-start block uniqueness after re-run (Windows-specific)
    if [ -f "$temp_home/.bashrc" ]; then
      local autostart_count
      autostart_count=$(grep -c '# Added by forge zsh setup' "$temp_home/.bashrc" 2>/dev/null || echo "0")
      if [ "$autostart_count" -eq 1 ]; then
        verify_output="${verify_output}
CHECK_EDGE_RERUN_BASHRC=PASS (still exactly 1 auto-start block)"
      else
        verify_output="${verify_output}
CHECK_EDGE_RERUN_BASHRC=FAIL (found ${autostart_count} auto-start blocks)"
      fi
    else
      verify_output="${verify_output}
CHECK_EDGE_RERUN_BASHRC=FAIL (no .bashrc after re-run)"
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
  if [ -n "$parsed_setup_exit" ] && [ "$parsed_setup_exit" != "0" ]; then
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
    cp "$temp_home/.bashrc" "$diag_dir/bashrc" 2>/dev/null || true
    cp "$temp_home/.zshenv" "$diag_dir/zshenv" 2>/dev/null || true
    cp "$temp_home/.bash_profile" "$diag_dir/bash_profile" 2>/dev/null || true
    cp "$temp_home/.bash_login" "$diag_dir/bash_login" 2>/dev/null || true
    cp "$temp_home/.profile" "$diag_dir/profile" 2>/dev/null || true
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
    RESULTS_DIR="$PROJECT_ROOT/test-results-windows"
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

  log_header "Phase 3: Windows/Git Bash E2E Tests"
  log_info "Results dir: ${RESULTS_DIR}"
  log_info "Build target: ${BUILD_TARGET}"
  log_info "Git Bash: $(uname -s) $(uname -r)"
  echo ""

  # Run each scenario sequentially
  for entry in "${SCENARIOS[@]}"; do
    IFS='|' read -r _id label _test_type <<< "$entry"

    # Apply filter
    if [ -n "$FILTER_PATTERN" ] && ! echo "$label" | grep -qiE "$FILTER_PATTERN"; then
      continue
    fi
    if [ -n "$EXCLUDE_PATTERN" ] && echo "$label" | grep -qiE "$EXCLUDE_PATTERN"; then
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

  echo -e "${BOLD}${BLUE}Forge ZSH Setup - Windows/Git Bash E2E Test Suite${NC}"
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
