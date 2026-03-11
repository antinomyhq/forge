#!/bin/bash
# =============================================================================
# E2E test suite for shell-native provider authentication
#
# Tests the `forge provider auth-info` and `forge provider login` non-interactive
# CLI commands, and the `_forge_provider_auth` zsh shell function that drives
# the shell-native authentication flow (replacing crossterm/dialoguer).
#
# This test suite verifies:
#   1. CLI: `forge provider auth-info <id>` output format and correctness
#   2. CLI: `forge provider login <id> --auth-method ... --api-key ... --init-only`
#      runs non-interactively without crossterm (no BracketedPasteGuard crash)
#   3. ZSH: `_forge_provider_auth` correctly assembles CLI arguments from
#      shell-native prompts (mocked fzf + read) and calls the CLI
#
# Platforms:
#   - Linux (Docker containers): full test suite including zsh function tests
#   - macOS: full test suite on native zsh
#   - Windows (Git Bash): CLI-only tests proving no crossterm crash on mintty
#
# Usage:
#   bash crates/forge_ci/tests/scripts/test-shell-auth.sh
#   bash crates/forge_ci/tests/scripts/test-shell-auth.sh --quick       # CLI tests only
#   bash crates/forge_ci/tests/scripts/test-shell-auth.sh --skip-build  # use existing binary
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

# Detect host platform
HOST_OS="$(uname -s)"
readonly HOST_OS

HOST_ARCH="$(uname -m)"
readonly HOST_ARCH

# =============================================================================
# Runtime state
# =============================================================================

PASS=0
FAIL=0
SKIP=0
FAILURES=()

MODE="full"          # full | quick (CLI-only, no zsh function tests)
SKIP_BUILD=false
RESULTS_DIR=""

# =============================================================================
# Logging helpers
# =============================================================================

log_header() { echo -e "\n${BOLD}${BLUE}$1${NC}"; }
log_pass()   { echo -e "  ${GREEN}PASS${NC} $1"; PASS=$((PASS + 1)); }
log_fail()   { echo -e "  ${RED}FAIL${NC} $1"; FAIL=$((FAIL + 1)); FAILURES+=("$1"); }
log_skip()   { echo -e "  ${YELLOW}SKIP${NC} $1"; SKIP=$((SKIP + 1)); }
log_info()   { echo -e "  ${DIM}$1${NC}"; }

check_pass() { local name="$1" detail="${2:-}"; echo "CHECK_${name}=PASS ${detail}"; }
check_fail() { local name="$1" detail="${2:-}"; echo "CHECK_${name}=FAIL ${detail}"; }

# =============================================================================
# Argument parsing
# =============================================================================

print_usage() {
  cat <<EOF
Usage: bash crates/forge_ci/tests/scripts/test-shell-auth.sh [OPTIONS]

Options:
  --quick              CLI tests only (skip zsh function tests)
  --skip-build         Use existing binary (skip cargo build)
  --help               Show this help message

Environment variables:
  FORGE_BINARY         Path to forge binary (overrides default)
EOF
}

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --quick)   MODE="quick"; shift ;;
      --skip-build) SKIP_BUILD=true; shift ;;
      --help|-h) print_usage; exit 0 ;;
      *) echo "Unknown option: $1" >&2; print_usage >&2; exit 1 ;;
    esac
  done
}

# =============================================================================
# Binary setup
# =============================================================================

FORGE_BIN=""

setup_binary() {
  log_header "Phase 1: Binary Setup"

  # Allow override via environment variable
  if [ -n "${FORGE_BINARY:-}" ]; then
    if [ -f "$FORGE_BINARY" ] && [ -x "$FORGE_BINARY" ]; then
      FORGE_BIN="$FORGE_BINARY"
      log_pass "Using FORGE_BINARY: $FORGE_BIN"
      return 0
    else
      log_fail "FORGE_BINARY set but not found/executable: $FORGE_BINARY"
      return 1
    fi
  fi

  # Determine binary path based on platform
  local binary_path
  if [ "$HOST_OS" = "Windows_NT" ] || echo "$HOST_OS" | grep -qi "mingw\|msys\|cygwin"; then
    binary_path="$PROJECT_ROOT/target/debug/forge.exe"
  else
    binary_path="$PROJECT_ROOT/target/debug/forge"
  fi

  if [ "$SKIP_BUILD" = true ] && [ -f "$binary_path" ]; then
    FORGE_BIN="$binary_path"
    log_pass "Using existing binary: $FORGE_BIN"
    return 0
  fi

  log_info "Building forge (debug)..."
  if ! cargo build --manifest-path "$PROJECT_ROOT/Cargo.toml" 2>"$RESULTS_DIR/build.log"; then
    log_fail "cargo build failed"
    log_info "Build log: $RESULTS_DIR/build.log"
    cat "$RESULTS_DIR/build.log" >&2
    return 1
  fi

  if [ ! -f "$binary_path" ]; then
    log_fail "Binary not found after build: $binary_path"
    return 1
  fi

  FORGE_BIN="$binary_path"
  log_pass "Built: $FORGE_BIN ($(du -h "$FORGE_BIN" | cut -f1))"
}

# =============================================================================
# Phase 2: CLI auth-info tests
# =============================================================================

run_cli_auth_info_tests() {
  log_header "Phase 2: CLI auth-info Tests"

  # Test 1: auth-info for a known single-method provider (anthropic)
  local output
  output=$("$FORGE_BIN" provider auth-info anthropic 2>&1) || true

  if echo "$output" | grep -q "^auth_methods="; then
    check_pass "AUTH_INFO_HAS_AUTH_METHODS" "$(echo "$output" | grep "^auth_methods=")"
    log_pass "auth-info has auth_methods field"
  else
    check_fail "AUTH_INFO_HAS_AUTH_METHODS" "missing auth_methods line"
    log_fail "auth-info missing auth_methods field"
  fi

  if echo "$output" | grep -q "^url_params="; then
    check_pass "AUTH_INFO_HAS_URL_PARAMS" "$(echo "$output" | grep "^url_params=")"
    log_pass "auth-info has url_params field"
  else
    check_fail "AUTH_INFO_HAS_URL_PARAMS" "missing url_params line"
    log_fail "auth-info missing url_params field"
  fi

  if echo "$output" | grep -q "^configured="; then
    check_pass "AUTH_INFO_HAS_CONFIGURED" "$(echo "$output" | grep "^configured=")"
    log_pass "auth-info has configured field"
  else
    check_fail "AUTH_INFO_HAS_CONFIGURED" "missing configured line"
    log_fail "auth-info missing configured field"
  fi

  # Test 2: anthropic has api_key as auth method
  if echo "$output" | grep -q "^auth_methods=api_key"; then
    check_pass "AUTH_INFO_ANTHROPIC_API_KEY" "auth_methods=api_key"
    log_pass "anthropic auth method is api_key"
  else
    check_fail "AUTH_INFO_ANTHROPIC_API_KEY" "expected api_key, got: $(echo "$output" | grep "^auth_methods=" || echo "not found")"
    log_fail "anthropic auth method should be api_key"
  fi

  # Test 3: anthropic has no url_params
  if echo "$output" | grep -q "^url_params=$"; then
    check_pass "AUTH_INFO_ANTHROPIC_NO_URL_PARAMS" "url_params empty"
    log_pass "anthropic has no url_params"
  else
    check_fail "AUTH_INFO_ANTHROPIC_NO_URL_PARAMS" "expected empty url_params"
    log_fail "anthropic should have no url_params"
  fi

  # Test 4: unknown provider outputs an error message
  # Note: the CLI logs errors but exits 0 (standard forge error handling pattern)
  local unknown_output
  unknown_output=$("$FORGE_BIN" provider auth-info nonexistent-provider-xyz-abc 2>&1) || true
  if echo "$unknown_output" | grep -qi "not found\|error\|unknown"; then
    check_pass "AUTH_INFO_UNKNOWN_PROVIDER_ERROR" "error message present"
    log_pass "unknown provider outputs error message"
  else
    check_fail "AUTH_INFO_UNKNOWN_PROVIDER_ERROR" "expected error message for unknown provider, got: $unknown_output"
    log_fail "unknown provider should output error message"
  fi

  # Test 5: configured=no when not logged in (after logout to ensure clean state)
  "$FORGE_BIN" provider logout anthropic 2>/dev/null || true
  local info_after_logout
  info_after_logout=$("$FORGE_BIN" provider auth-info anthropic 2>&1) || true
  if echo "$info_after_logout" | grep -q "^configured=no"; then
    check_pass "AUTH_INFO_CONFIGURED_NO_AFTER_LOGOUT" "configured=no"
    log_pass "configured=no after logout"
  else
    check_fail "AUTH_INFO_CONFIGURED_NO_AFTER_LOGOUT" "expected configured=no"
    log_fail "should show configured=no after logout"
  fi
}

# =============================================================================
# Phase 3: CLI non-interactive login tests
# =============================================================================

run_cli_login_tests() {
  log_header "Phase 3: CLI Non-Interactive Login Tests"

  # Ensure clean state
  "$FORGE_BIN" provider logout anthropic 2>/dev/null || true

  # Test 1: login with --init-only (no model fetch, no terminal needed)
  local output exit_code=0
  output=$("$FORGE_BIN" provider login anthropic \
    --auth-method api-key \
    --api-key "sk-ant-test-key-for-ci-testing" \
    --init-only 2>&1) || exit_code=$?

  if echo "$output" | grep -qi "configured successfully\|Anthropic configured"; then
    check_pass "LOGIN_INIT_ONLY_SUCCESS_MSG" "got success message"
    log_pass "login --init-only shows success message"
  else
    check_fail "LOGIN_INIT_ONLY_SUCCESS_MSG" "no success message: $output"
    log_fail "login --init-only should show success message"
  fi

  # Test 2: after login, configured=yes
  local info_after_login
  info_after_login=$("$FORGE_BIN" provider auth-info anthropic 2>&1) || true
  if echo "$info_after_login" | grep -q "^configured=yes"; then
    check_pass "LOGIN_CONFIGURED_YES_AFTER_LOGIN" "configured=yes"
    log_pass "configured=yes after login"
  else
    check_fail "LOGIN_CONFIGURED_YES_AFTER_LOGIN" "expected configured=yes, got: $info_after_login"
    log_fail "should show configured=yes after login"
  fi

  # Test 3: existing_api_key is masked (shows prefix and suffix)
  if echo "$info_after_login" | grep -q "^existing_api_key="; then
    local masked_key
    masked_key=$(echo "$info_after_login" | grep "^existing_api_key=" | cut -d= -f2)
    if echo "$masked_key" | grep -qE "^sk-\.\.\.[a-z0-9]+$"; then
      check_pass "LOGIN_API_KEY_MASKED" "masked: $masked_key"
      log_pass "API key is masked in auth-info output"
    else
      check_fail "LOGIN_API_KEY_MASKED" "unexpected mask format: $masked_key"
      log_fail "API key masking format unexpected"
    fi
  else
    check_pass "LOGIN_API_KEY_MASKED" "no existing_api_key shown (not configured or no masking)"
    log_skip "existing_api_key field not present"
  fi

  # Test 4: --set-active flag sets provider as active (no terminal prompt)
  local set_active_output exit_code_sa=0
  set_active_output=$("$FORGE_BIN" provider login anthropic \
    --auth-method api-key \
    --api-key "sk-ant-test-key-for-ci-testing" \
    --set-active \
    --init-only 2>&1) || exit_code_sa=$?

  if echo "$set_active_output" | grep -qi "default provider\|now the default\|set as active\|configured successfully"; then
    check_pass "LOGIN_SET_ACTIVE_MSG" "got activation message"
    log_pass "login --set-active shows activation message"
  else
    check_fail "LOGIN_SET_ACTIVE_MSG" "no activation message: $set_active_output"
    log_fail "login --set-active should show activation message"
  fi

  # Test 5: no crossterm/Windows mintty errors in non-interactive mode
  # This is the core regression test for the Windows mintty issue:
  # "IO error: Incorrect function (os error 1)" from BracketedPasteGuard::new()
  # Note: "not a terminal" is a benign dialoguer error that is acceptable here
  if echo "$output$set_active_output" | grep -qi "incorrect function\|bracketedpaste\|os error 1"; then
    check_fail "LOGIN_NO_CROSSTERM_ERRORS" "found Windows mintty crossterm errors in output"
    log_fail "Windows mintty crossterm errors detected in non-interactive login"
  else
    check_pass "LOGIN_NO_CROSSTERM_ERRORS" "no Windows mintty crossterm errors"
    log_pass "no Windows mintty crossterm errors in non-interactive login"
  fi

  # Test 6: invalid auth method returns non-zero exit
  local invalid_exit=0
  "$FORGE_BIN" provider login anthropic \
    --auth-method invalid-method-xyz \
    --api-key "sk-test" \
    --init-only 2>/dev/null || invalid_exit=$?
  if [ "$invalid_exit" -ne 0 ]; then
    check_pass "LOGIN_INVALID_AUTH_METHOD_ERROR" "exit=$invalid_exit"
    log_pass "invalid auth method returns non-zero exit"
  else
    check_fail "LOGIN_INVALID_AUTH_METHOD_ERROR" "expected non-zero exit"
    log_fail "invalid auth method should return non-zero exit"
  fi

  # Cleanup: logout after tests
  "$FORGE_BIN" provider logout anthropic 2>/dev/null || true

  # Test 7: logout works and configured=no after logout
  local info_after_logout2
  info_after_logout2=$("$FORGE_BIN" provider auth-info anthropic 2>&1) || true
  if echo "$info_after_logout2" | grep -q "^configured=no"; then
    check_pass "LOGOUT_CONFIGURED_NO" "configured=no after logout"
    log_pass "configured=no after logout"
  else
    check_fail "LOGOUT_CONFIGURED_NO" "expected configured=no"
    log_fail "should show configured=no after logout"
  fi
}

# =============================================================================
# Phase 4: ZSH shell function tests (Linux/macOS only)
# =============================================================================

# Generate the in-process zsh test script that sources the forge plugin
# and tests _forge_provider_auth with mocked fzf and _forge_exec
generate_zsh_test_script() {
  local forge_bin="$1"

  cat <<HEREDOC
#!/usr/bin/env zsh
# ZSH shell function tests for _forge_provider_auth
# Uses mocked fzf and _forge_exec to verify argument construction

# Initialize zsh completion system so compdef works (needed by forge zsh plugin)
autoload -Uz compinit 2>/dev/null
compinit -u 2>/dev/null || true

# =============================================================================
# Setup: Source the forge zsh plugin
# =============================================================================

# Generate and source the plugin (suppress compdef warnings from non-interactive zsh)
eval "\$('${forge_bin}' zsh plugin 2>/dev/null)" 2>/dev/null
if [[ -z "\$_FORGE_PLUGIN_LOADED" ]]; then
  echo "CHECK_ZSH_PLUGIN_LOAD=FAIL _FORGE_PLUGIN_LOADED not set after eval"
  exit 1
fi
echo "CHECK_ZSH_PLUGIN_LOAD=PASS plugin loaded (timestamp=\$_FORGE_PLUGIN_LOADED)"

# =============================================================================
# Mock infrastructure
# =============================================================================

# Mock fzf: always selects the first line of input
function fzf() {
  head -1
}

# Capture directory for _forge_exec calls
CAPTURE_DIR="\$(mktemp -d)"
CALL_COUNT=0

# Mock _forge_exec: captures args to file, returns pre-canned auth-info output
# Note: we do NOT forward auth-info to the real binary because the real binary
# consumes stdin (for REPL mode), which would steal the piped API key input
# before 'read -rs' can read it. Instead, return pre-canned output.
function _forge_exec() {
  CALL_COUNT=\$((CALL_COUNT + 1))
  echo "\$@" > "\${CAPTURE_DIR}/call_\${CALL_COUNT}.txt"

  # Return pre-canned auth-info for anthropic (api_key method, no url_params)
  if [[ "\$1" == "provider" && "\$2" == "auth-info" && "\$3" == "anthropic" ]]; then
    echo "auth_methods=api_key"
    echo "url_params="
    echo "configured=no"
    return 0
  fi

  # For login calls, just print success
  if [[ "\$1" == "provider" && "\$2" == "login" ]]; then
    echo "configured successfully (mocked)"
    return 0
  fi

  return 0
}

# =============================================================================
# Test 1: _forge_provider_auth constructs correct args for api_key provider
# =============================================================================

echo ""
echo "--- Test: api_key auth method arg construction ---"

# Ensure anthropic is logged out for clean state
'${forge_bin}' provider logout anthropic 2>/dev/null || true

# Reset capture
CALL_COUNT=0
rm -f "\${CAPTURE_DIR}"/call_*.txt

# Simulate user input: API key via stdin pipe
# _forge_provider_auth reads API key with 'read -rs' from stderr-redirected prompt
# We simulate this by piping to the function via a subshell with stdin
(
  # Pipe the fake API key as stdin for the 'read -rs' call
  echo "sk-ant-test-key-from-shell"
) | _forge_provider_auth "anthropic" 2>/dev/null
auth_exit=\$?

# Find the login call (should be call #2 after auth-info)
login_call_file=""
for f in "\${CAPTURE_DIR}"/call_*.txt; do
  if grep -q "provider login" "\$f" 2>/dev/null; then
    login_call_file="\$f"
    break
  fi
done

if [[ -n "\$login_call_file" ]]; then
  login_args="\$(cat "\$login_call_file")"

  # Check --auth-method api-key is present
  if echo "\$login_args" | grep -q "\-\-auth-method api-key"; then
    echo "CHECK_ZSH_AUTH_METHOD_ARG=PASS --auth-method api-key present"
  else
    echo "CHECK_ZSH_AUTH_METHOD_ARG=FAIL missing --auth-method api-key in: \$login_args"
  fi

  # Check --api-key is present with a value
  if echo "\$login_args" | grep -q "\-\-api-key"; then
    echo "CHECK_ZSH_API_KEY_ARG=PASS --api-key present"
  else
    echo "CHECK_ZSH_API_KEY_ARG=FAIL missing --api-key in: \$login_args"
  fi

  # Check --set-active is present
  if echo "\$login_args" | grep -q "\-\-set-active"; then
    echo "CHECK_ZSH_SET_ACTIVE_ARG=PASS --set-active present"
  else
    echo "CHECK_ZSH_SET_ACTIVE_ARG=FAIL missing --set-active in: \$login_args"
  fi

  # Check provider ID is correct
  if echo "\$login_args" | grep -q "anthropic"; then
    echo "CHECK_ZSH_PROVIDER_ID_ARG=PASS provider id anthropic present"
  else
    echo "CHECK_ZSH_PROVIDER_ID_ARG=FAIL missing provider id in: \$login_args"
  fi
else
  echo "CHECK_ZSH_AUTH_METHOD_ARG=FAIL no login call captured"
  echo "CHECK_ZSH_API_KEY_ARG=FAIL no login call captured"
  echo "CHECK_ZSH_SET_ACTIVE_ARG=FAIL no login call captured"
  echo "CHECK_ZSH_PROVIDER_ID_ARG=FAIL no login call captured"
fi

# =============================================================================
# Test 2: _forge_provider_auth uses kebab-case for auth method (not underscore)
# =============================================================================

echo ""
echo "--- Test: auth method is kebab-case in CLI args ---"

if [[ -n "\$login_call_file" ]]; then
  login_args="\$(cat "\$login_call_file")"
  # Should be api-key (kebab), NOT api_key (underscore)
  if echo "\$login_args" | grep -q "\-\-auth-method api-key" && ! echo "\$login_args" | grep -q "\-\-auth-method api_key"; then
    echo "CHECK_ZSH_KEBAB_CASE=PASS api-key (not api_key)"
  else
    echo "CHECK_ZSH_KEBAB_CASE=FAIL wrong format in: \$login_args"
  fi
else
  echo "CHECK_ZSH_KEBAB_CASE=FAIL no login call to check"
fi

# =============================================================================
# Test 3: _forge_action_login selects provider via fzf then calls _forge_provider_auth
# =============================================================================

echo ""
echo "--- Test: _forge_action_login flow ---"

# Reset capture
CALL_COUNT=0
rm -f "\${CAPTURE_DIR}"/call_*.txt

# _forge_action_login calls _forge_select_provider (which uses fzf)
# Our mock fzf returns the first line, so we need _forge_select_provider to work
# We override _forge_select_provider to return a known provider line
function _forge_select_provider() {
  echo "Anthropic                  anthropic                    [empty]            llm"
}

# Run _forge_action_login with piped input for the API key read
(echo "sk-ant-action-login-test") | _forge_action_login "" 2>/dev/null
action_exit=\$?

# Check that a login call was made
login_found=false
for f in "\${CAPTURE_DIR}"/call_*.txt; do
  if grep -q "provider login" "\$f" 2>/dev/null; then
    login_found=true
    break
  fi
done

if [[ "\$login_found" == "true" ]]; then
  echo "CHECK_ZSH_ACTION_LOGIN_CALLS_PROVIDER_AUTH=PASS login call captured"
else
  echo "CHECK_ZSH_ACTION_LOGIN_CALLS_PROVIDER_AUTH=FAIL no login call captured"
fi

# =============================================================================
# Cleanup
# =============================================================================

rm -rf "\${CAPTURE_DIR}"
echo ""
echo "ZSH tests complete"
HEREDOC
}

run_zsh_function_tests() {
  log_header "Phase 4: ZSH Shell Function Tests"

  # Check if zsh is available
  if ! command -v zsh > /dev/null 2>&1; then
    log_skip "zsh not available — skipping shell function tests"
    return 0
  fi

  local zsh_script
  zsh_script=$(generate_zsh_test_script "$FORGE_BIN")

  # Write the script to a temp file — zsh behaves differently when run as a
  # script file vs receiving stdin (compinit and eval work correctly in file mode)
  local zsh_script_file
  zsh_script_file=$(mktemp "$RESULTS_DIR/zsh_test_XXXXXX.zsh")
  echo "$zsh_script" > "$zsh_script_file"
  chmod +x "$zsh_script_file"

  # Run the zsh test script
  local zsh_output exit_code=0
  zsh_output=$(zsh "$zsh_script_file" 2>&1) || exit_code=$?

  # Parse CHECK_* lines from zsh output
  while IFS= read -r line; do
    case "$line" in
      CHECK_*=PASS*)
        local name
        name=$(echo "$line" | cut -d= -f1 | sed 's/CHECK_//')
        local detail
        detail=$(echo "$line" | cut -d' ' -f2-)
        log_pass "ZSH: $name — $detail"
        ;;
      CHECK_*=FAIL*)
        local name
        name=$(echo "$line" | cut -d= -f1 | sed 's/CHECK_//')
        local detail
        detail=$(echo "$line" | cut -d' ' -f2-)
        log_fail "ZSH: $name — $detail"
        ;;
    esac
  done <<< "$zsh_output"

  if [ "$exit_code" -ne 0 ]; then
    log_fail "ZSH test script exited with code $exit_code"
    log_info "ZSH output: $zsh_output"
  fi
}

# =============================================================================
# Phase 5: Windows-specific regression tests
# =============================================================================

run_windows_regression_tests() {
  log_header "Phase 5: Windows/mintty Regression Tests"

  # These tests verify the core bug fix: no crossterm errors on non-TTY
  # The key symptom was: "IO error: Incorrect function (os error 1)"
  # caused by BracketedPasteGuard::new() calling execute!(stdout(), DisableBracketedPaste)

  # Test: provider auth-info works without a TTY
  local output exit_code=0
  output=$("$FORGE_BIN" provider auth-info anthropic 2>&1 </dev/null) || exit_code=$?

  if [ "$exit_code" -eq 0 ]; then
    check_pass "WINDOWS_AUTH_INFO_NO_TTY" "exit=0 without TTY"
    log_pass "auth-info works without TTY (no crossterm crash)"
  else
    check_fail "WINDOWS_AUTH_INFO_NO_TTY" "exit=$exit_code without TTY"
    log_fail "auth-info failed without TTY"
  fi

  if echo "$output" | grep -qi "incorrect function\|os error 1\|bracketedpaste"; then
    check_fail "WINDOWS_NO_BRACKETEDPASTE_ERROR" "found BracketedPaste error: $output"
    log_fail "BracketedPaste error detected (Windows mintty regression)"
  else
    check_pass "WINDOWS_NO_BRACKETEDPASTE_ERROR" "no BracketedPaste errors"
    log_pass "no BracketedPaste errors (Windows mintty regression test passed)"
  fi

  # Test: login with all args works without a TTY (piped stdin)
  local login_output login_exit=0
  login_output=$("$FORGE_BIN" provider login anthropic \
    --auth-method api-key \
    --api-key "sk-ant-windows-test-key" \
    --init-only 2>&1 </dev/null) || login_exit=$?

  if echo "$login_output" | grep -qi "incorrect function\|os error 1\|bracketedpaste"; then
    check_fail "WINDOWS_LOGIN_NO_BRACKETEDPASTE" "found BracketedPaste error"
    log_fail "BracketedPaste error in login (Windows mintty regression)"
  else
    check_pass "WINDOWS_LOGIN_NO_BRACKETEDPASTE" "no BracketedPaste errors"
    log_pass "login --init-only has no BracketedPaste errors"
  fi

  # Cleanup
  "$FORGE_BIN" provider logout anthropic 2>/dev/null || true
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
      echo -e "  ${RED}✗${NC} $f"
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
# Main
# =============================================================================

main() {
  parse_args "$@"

  # Setup results directory
  RESULTS_DIR=$(mktemp -d)
  trap 'rm -rf "$RESULTS_DIR"' EXIT

  echo ""
  echo -e "${BOLD}${BLUE}Shell-Native Provider Auth E2E Tests${NC}"
  echo -e "${DIM}Platform: $HOST_OS/$HOST_ARCH${NC}"
  echo -e "${DIM}Mode: $MODE${NC}"
  echo ""

  # Phase 1: Build/find binary
  if ! setup_binary; then
    echo "Error: Cannot proceed without forge binary" >&2
    exit 1
  fi

  # Phase 2: CLI auth-info tests (all platforms)
  run_cli_auth_info_tests

  # Phase 3: CLI non-interactive login tests (all platforms)
  run_cli_login_tests

  # Phase 4: ZSH function tests (Linux/macOS only, not --quick)
  if [ "$MODE" != "quick" ]; then
    case "$HOST_OS" in
      Linux|Darwin)
        run_zsh_function_tests
        ;;
      *)
        log_skip "ZSH function tests (platform: $HOST_OS)"
        ;;
    esac
  else
    log_skip "ZSH function tests (--quick mode)"
  fi

  # Phase 5: Windows regression tests (all platforms — verifies no crossterm crash)
  run_windows_regression_tests

  # Print summary and exit with appropriate code
  print_summary
}

main "$@"
