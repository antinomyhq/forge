#!/usr/bin/env bash
# scripts/test-reasoning.sh
#
# Validates that reasoning parameters are correctly serialized for each
# provider. Runs all entries in docs/reasoning-test.md plus Chat Completions
# (GitHub Copilot) and Responses API (Codex) paths.
#
# Usage: ./scripts/test-reasoning.sh

set -uo pipefail

# ─── colors ───────────────────────────────────────────────────────────────────

BOLD='\033[1m'
RESET='\033[0m'
GREEN='\033[32m'
RED='\033[31m'
YELLOW='\033[33m'
CYAN='\033[36m'
DIM='\033[2m'

# ─── state ────────────────────────────────────────────────────────────────────

PASS=0
FAIL=0
SKIP=0
BINARY="target/debug/forge"
WORK_DIR="$(mktemp -d)"

cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

# ─── output helpers ───────────────────────────────────────────────────────────

log_header() { printf "\n${BOLD}${CYAN}▶  %s${RESET}\n" "$1"; }
log_pass()   { printf "  ${GREEN}✓${RESET}  %s\n" "$1"; PASS=$((PASS + 1)); }
log_fail()   { printf "  ${RED}✗${RESET}  %s\n" "$1"; FAIL=$((FAIL + 1)); }
log_skip()   { printf "  ${YELLOW}~${RESET}  %s\n" "$1"; SKIP=$((SKIP + 1)); }

# ─── json helpers ─────────────────────────────────────────────────────────────

# json_get <file> <dot.separated.path>
# Prints the JSON value at the given path, or "null" if absent/null.
json_get() {
    python3 - "$1" "$2" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    d = json.load(f)
keys = sys.argv[2].split('.')
v = d
for k in keys:
    v = v.get(k) if isinstance(v, dict) else None
    if v is None:
        break
print(json.dumps(v))
PY
}

# assert_field <file> <dot.path> <expected_json_value> <label>
assert_field() {
    local file="$1" path="$2" expected="$3" label="$4"
    local actual
    actual="$(json_get "$file" "$path")"
    if [ "$actual" = "$expected" ]; then
        log_pass "$label  $path = $expected"
    else
        log_fail "$label  $path — expected $expected, got $actual"
    fi
}

# ─── test runner ──────────────────────────────────────────────────────────────

# run_test <outfile> <provider_id> <model_id> [KEY=VALUE ...]
# Runs forge with FORGE_DEBUG_REQUESTS pointing to <outfile>.
# Extra KEY=VALUE arguments are forwarded as additional env vars.
# Returns 0 if the request file was written, 1 otherwise (e.g. auth missing).
run_test() {
    local out="$1" provider="$2" model="$3"
    shift 3

    env FORGE_DEBUG_REQUESTS="$out" \
        FORGE_SESSION__PROVIDER_ID="$provider" \
        FORGE_SESSION__MODEL_ID="$model" \
        "$@" \
        "$BINARY" -p "Hello!" >/dev/null 2>&1 || true

    [ -f "$out" ]
}

# ─── build ────────────────────────────────────────────────────────────────────

printf "${BOLD}Reasoning Serialization Tests${RESET}\n"
printf "${DIM}Building forge (debug)...${RESET}\n\n"
if ! cargo build 2>&1 | grep -E "^error|Finished|^   Compiling forge_main"; then
    printf "${RED}Build failed — aborting.${RESET}\n"
    exit 1
fi

# ─── test 1: OpenRouter + OpenAI o4-mini ──────────────────────────────────────
# Expected: reasoning.effort = "high", no max_tokens/enabled bleed-through

log_header "OpenRouter · openai/o4-mini · effort: high"
OUT="$WORK_DIR/openrouter-openai.json"
if run_test "$OUT" open_router "openai/o4-mini" FORGE_REASONING__EFFORT=high; then
    assert_field "$OUT" "reasoning.effort"     '"high"' "openrouter/openai"
    assert_field "$OUT" "reasoning.max_tokens" "null"   "openrouter/openai"
    assert_field "$OUT" "reasoning.enabled"    "null"   "openrouter/openai"
else
    log_skip "open_router not configured — skipping"
fi

# ─── test 2: OpenRouter + Anthropic claude-opus-4-5 ──────────────────────────
# Expected: reasoning.max_tokens = 4000, no effort/enabled bleed-through

log_header "OpenRouter · anthropic/claude-opus-4-5 · max_tokens: 4000"
OUT="$WORK_DIR/openrouter-anthropic.json"
if run_test "$OUT" open_router "anthropic/claude-opus-4-5" FORGE_REASONING__MAX_TOKENS=4000; then
    assert_field "$OUT" "reasoning.max_tokens" "4000"  "openrouter/anthropic"
    assert_field "$OUT" "reasoning.effort"     "null"  "openrouter/anthropic"
    assert_field "$OUT" "reasoning.enabled"    "null"  "openrouter/anthropic"
else
    log_skip "open_router not configured — skipping"
fi

# ─── test 3: Anthropic claude-opus-4-6 (newer model → output_config.effort) ──
# Expected: output_config.effort = "medium", no thinking object

log_header "Anthropic · claude-opus-4-6 · effort: medium"
OUT="$WORK_DIR/anthropic-opus46.json"
if run_test "$OUT" anthropic "claude-opus-4-6" FORGE_REASONING__EFFORT=medium; then
    assert_field "$OUT" "output_config.effort" '"medium"' "anthropic/opus4.6"
    assert_field "$OUT" "thinking"             "null"     "anthropic/opus4.6"
else
    log_skip "anthropic not configured — skipping"
fi

# ─── test 4: Anthropic claude-3-7-sonnet (older model → thinking object) ─────
# Expected: thinking.type = "enabled", thinking.budget_tokens = 8000, no output_config

log_header "Anthropic · claude-3-7-sonnet-20250219 · enabled: true + max_tokens: 8000"
OUT="$WORK_DIR/anthropic-sonnet37.json"
if run_test "$OUT" anthropic "claude-3-7-sonnet-20250219" \
        FORGE_REASONING__ENABLED=true FORGE_REASONING__MAX_TOKENS=8000; then
    assert_field "$OUT" "thinking.type"          '"enabled"' "anthropic/sonnet3.7"
    assert_field "$OUT" "thinking.budget_tokens" "8000"      "anthropic/sonnet3.7"
    assert_field "$OUT" "output_config"          "null"      "anthropic/sonnet3.7"
else
    log_skip "anthropic not configured — skipping"
fi

# ─── test 5: GitHub Copilot + o4-mini (Chat Completions → reasoning_effort) ───
# Expected: top-level reasoning_effort = "medium", no reasoning object

log_header "GitHub Copilot · o4-mini · effort: medium"
OUT="$WORK_DIR/github-copilot-o4mini.json"
if run_test "$OUT" github_copilot "o4-mini" FORGE_REASONING__EFFORT=medium; then
    assert_field "$OUT" "reasoning_effort" '"medium"' "github_copilot/o4-mini"
    assert_field "$OUT" "reasoning"        "null"     "github_copilot/o4-mini"
else
    log_skip "github_copilot not configured — skipping"
fi

# ─── test 6: Codex + gpt-5.1-codex (Responses API → reasoning object) ────────
# Expected: reasoning.effort = "medium" (user-specified), reasoning.summary = "auto" (default)

log_header "Codex · gpt-5.1-codex · effort: medium"
OUT="$WORK_DIR/codex-gpt51.json"
if run_test "$OUT" codex "gpt-5.1-codex" FORGE_REASONING__EFFORT=medium; then
    assert_field "$OUT" "reasoning.effort"  '"medium"' "codex/gpt-5.1-codex"
    assert_field "$OUT" "reasoning.summary" '"auto"'   "codex/gpt-5.1-codex"
    assert_field "$OUT" "reasoning_effort"  "null"     "codex/gpt-5.1-codex"
else
    log_skip "codex not configured — skipping"
fi

# ─── summary ──────────────────────────────────────────────────────────────────

printf "\n${BOLD}─────────────────────────────────────────${RESET}\n"
printf "${BOLD}Results:${RESET}  "
printf "${GREEN}%d passed${RESET}  " "$PASS"
[ "$FAIL" -gt 0 ] && printf "${RED}%d failed${RESET}  " "$FAIL" \
                  || printf "${DIM}%d failed${RESET}  " "$FAIL"
printf "${YELLOW}%d skipped${RESET}\n\n" "$SKIP"

[ "$FAIL" -eq 0 ]
