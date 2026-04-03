#!/usr/bin/env zsh

# Correctness and performance tests for zsh plugin utility functions.
# Focuses on _forge_wrap_file_paths which wraps bare file paths in @[...]
# syntax for the forge shell plugin.
#
# Usage: zsh scripts/test-zsh-utils.sh

set -euo pipefail
zmodload zsh/datetime

# Colors
BOLD='\033[1m'
DIM='\033[2m'
RESET='\033[0m'
GREEN='\033[32m'
RED='\033[31m'
YELLOW='\033[33m'
CYAN='\033[36m'
GRAY='\033[90m'

PASS=0
FAIL=0

# Source the helpers that define _forge_wrap_file_paths
SCRIPT_DIR="${0:A:h}"
source "${SCRIPT_DIR}/../shell-plugin/lib/helpers.zsh"

# Create temporary files for testing paths with spaces
TMPDIR_TEST=$(mktemp -d)
mkdir -p "${TMPDIR_TEST}/my folder"
touch "${TMPDIR_TEST}/my folder/test file.txt"
touch "${TMPDIR_TEST}/simple.txt"

# --- Test harness -----------------------------------------------------------

function assert_eq() {
    local test_name="$1"
    local actual="$2"
    local expected="$3"

    if [[ "$actual" == "$expected" ]]; then
        printf "  ${GREEN}✓${RESET} %s\n" "$test_name"
        PASS=$(( PASS + 1 ))
    else
        printf "  ${RED}✗${RESET} %s\n" "$test_name"
        printf "    ${DIM}expected:${RESET} %s\n" "$expected"
        printf "    ${DIM}  actual:${RESET} %s\n" "$actual"
        FAIL=$(( FAIL + 1 ))
    fi
}

# --- Correctness tests ------------------------------------------------------

echo ""
echo -e "${BOLD}Correctness Tests${RESET} ${DIM}— _forge_wrap_file_paths${RESET}"
echo ""

# Basic wrapping
assert_eq "bare existing path" \
    "$(_forge_wrap_file_paths "/usr/bin/env")" \
    "@[/usr/bin/env]"

assert_eq "path in sentence" \
    "$(_forge_wrap_file_paths "look at /usr/bin/env please")" \
    "look at @[/usr/bin/env] please"

# Non-existent paths left untouched
assert_eq "nonexistent path untouched" \
    "$(_forge_wrap_file_paths "check /nonexistent/foo.rs")" \
    "check /nonexistent/foo.rs"

# Already wrapped left untouched
assert_eq "already wrapped @[...] untouched" \
    "$(_forge_wrap_file_paths "check @[/usr/bin/env] ok")" \
    "check @[/usr/bin/env] ok"

# Plain text (no paths)
assert_eq "plain text no paths" \
    "$(_forge_wrap_file_paths "hello world")" \
    "hello world"

# Paths with spaces
assert_eq "bare path with spaces" \
    "$(_forge_wrap_file_paths "${TMPDIR_TEST}/my folder/test file.txt")" \
    "@[${TMPDIR_TEST}/my folder/test file.txt]"

assert_eq "bare path with spaces in sentence" \
    "$(_forge_wrap_file_paths "check ${TMPDIR_TEST}/my folder/test file.txt")" \
    "check @[${TMPDIR_TEST}/my folder/test file.txt]"

# Quoted paths with spaces
assert_eq "single-quoted path with spaces" \
    "$(_forge_wrap_file_paths "'${TMPDIR_TEST}/my folder/test file.txt'")" \
    "@[${TMPDIR_TEST}/my folder/test file.txt]"

assert_eq "double-quoted path with spaces" \
    "$(_forge_wrap_file_paths "\"${TMPDIR_TEST}/my folder/test file.txt\"")" \
    "@[${TMPDIR_TEST}/my folder/test file.txt]"

assert_eq "single-quoted path with spaces in sentence" \
    "$(_forge_wrap_file_paths "check '${TMPDIR_TEST}/my folder/test file.txt' please")" \
    "check @[${TMPDIR_TEST}/my folder/test file.txt] please"

# Simple path (no spaces)
assert_eq "simple path no spaces" \
    "$(_forge_wrap_file_paths "${TMPDIR_TEST}/simple.txt")" \
    "@[${TMPDIR_TEST}/simple.txt]"

# Multiple paths
assert_eq "multiple existing paths" \
    "$(_forge_wrap_file_paths "compare /usr/bin/env and ${TMPDIR_TEST}/simple.txt")" \
    "compare @[/usr/bin/env] and @[${TMPDIR_TEST}/simple.txt]"

assert_eq "mixed existing and nonexistent" \
    "$(_forge_wrap_file_paths "check /usr/bin/env and /nonexistent/foo.rs")" \
    "check @[/usr/bin/env] and /nonexistent/foo.rs"

# Empty input
assert_eq "empty input" \
    "$(_forge_wrap_file_paths "")" \
    ""

# Backslash-escaped paths (terminals like Ghostty send /path/my\ file.txt)
local escaped_path="${TMPDIR_TEST}/my\ folder/test\ file.txt"
assert_eq "backslash-escaped path (whole paste)" \
    "$(_forge_wrap_file_paths "$escaped_path")" \
    "@[${TMPDIR_TEST}/my folder/test file.txt]"

assert_eq "backslash-escaped path in sentence" \
    "$(_forge_wrap_file_paths "check $escaped_path please")" \
    "check @[${TMPDIR_TEST}/my folder/test file.txt] please"

local escaped_simple="${TMPDIR_TEST}/simple.txt"
assert_eq "path without spaces (no escaping needed)" \
    "$(_forge_wrap_file_paths "$escaped_simple")" \
    "@[${TMPDIR_TEST}/simple.txt]"

assert_eq "backslash-escaped nonexistent path untouched" \
    "$(_forge_wrap_file_paths "/nonexistent/my\ folder/file.txt")" \
    "/nonexistent/my\ folder/file.txt"

# _forge_unescape_backslashes tests
assert_eq "unescape: backslash space" \
    "$(_forge_unescape_backslashes '/path/my\ file.txt')" \
    "/path/my file.txt"

assert_eq "unescape: no backslashes" \
    "$(_forge_unescape_backslashes "/path/file.txt")" \
    "/path/file.txt"

assert_eq "unescape: trailing backslash" \
    "$(_forge_unescape_backslashes '/path/file\')" \
    '/path/file\'

# --- Performance tests -------------------------------------------------------

echo ""
echo -e "${BOLD}Performance Tests${RESET} ${DIM}— _forge_wrap_file_paths${RESET}"
echo ""

ITERATIONS=100

# Benchmark: simple path
START=$EPOCHREALTIME
for i in $(seq 1 $ITERATIONS); do
    _forge_wrap_file_paths "look at /usr/bin/env please" > /dev/null
done
END=$EPOCHREALTIME
ELAPSED=$(( (END - START) * 1000 ))
AVG=$(( ELAPSED / ITERATIONS ))
printf "  ${DIM}simple path        ${RESET} ${CYAN}%.2f${RESET} ${DIM}ms avg (${ITERATIONS} iterations)${RESET}\n" $AVG

# Benchmark: path with spaces
START=$EPOCHREALTIME
for i in $(seq 1 $ITERATIONS); do
    _forge_wrap_file_paths "check '${TMPDIR_TEST}/my folder/test file.txt' please" > /dev/null
done
END=$EPOCHREALTIME
ELAPSED=$(( (END - START) * 1000 ))
AVG=$(( ELAPSED / ITERATIONS ))
printf "  ${DIM}quoted path spaces ${RESET} ${CYAN}%.2f${RESET} ${DIM}ms avg (${ITERATIONS} iterations)${RESET}\n" $AVG

# Benchmark: plain text (no paths)
START=$EPOCHREALTIME
for i in $(seq 1 $ITERATIONS); do
    _forge_wrap_file_paths "explain how this works in detail" > /dev/null
done
END=$EPOCHREALTIME
ELAPSED=$(( (END - START) * 1000 ))
AVG=$(( ELAPSED / ITERATIONS ))
printf "  ${DIM}plain text         ${RESET} ${CYAN}%.2f${RESET} ${DIM}ms avg (${ITERATIONS} iterations)${RESET}\n" $AVG

# Benchmark: already wrapped
START=$EPOCHREALTIME
for i in $(seq 1 $ITERATIONS); do
    _forge_wrap_file_paths "check @[/usr/bin/env] and explain" > /dev/null
done
END=$EPOCHREALTIME
ELAPSED=$(( (END - START) * 1000 ))
AVG=$(( ELAPSED / ITERATIONS ))
printf "  ${DIM}already wrapped    ${RESET} ${CYAN}%.2f${RESET} ${DIM}ms avg (${ITERATIONS} iterations)${RESET}\n" $AVG

# --- Cleanup -----------------------------------------------------------------

rm -rf "$TMPDIR_TEST"

# --- Summary -----------------------------------------------------------------

echo ""
TOTAL=$(( PASS + FAIL ))
if (( FAIL > 0 )); then
    echo -e "${RED}${BOLD}FAILED${RESET} ${PASS}/${TOTAL} passed, ${FAIL} failed"
    echo ""
    exit 1
else
    echo -e "${GREEN}${BOLD}ALL PASSED${RESET} ${PASS}/${TOTAL}"
    echo ""
fi
