#!/usr/bin/env bash
# Test that per-agent model config env vars correctly route requests to the
# overridden model, and that preset reasoning effort is sent in the POST body.
#
# Env var naming follows ForgeConfig field names (crates/forge_config/src/config.rs)
# with the FORGE_ prefix and __ separator for nested ModelConfig fields:
#
#   config field               env var
#   ─────────────────────────  ──────────────────────────────────
#   session.model_id           FORGE_SESSION__MODEL_ID
#   agent_forge.model_id       FORGE_AGENT_FORGE__MODEL_ID
#   agent_forge.preset_id      FORGE_AGENT_FORGE__PRESET_ID
#   agent_muse.model_id        FORGE_AGENT_MUSE__MODEL_ID
#   agent_muse.preset_id       FORGE_AGENT_MUSE__PRESET_ID
#   agent_sage.model_id        FORGE_AGENT_SAGE__MODEL_ID
#   agent_sage.preset_id       FORGE_AGENT_SAGE__PRESET_ID
#   agent_sage.provider_id     FORGE_AGENT_SAGE__PROVIDER_ID
#
# Note: presets.* (HashMap) cannot be set via env vars — they must be in a TOML
# config file. This script appends test presets to ~/forge/.forge.toml and
# removes them on exit.

set -euo pipefail

BINARY="./target/debug/forge"
OVERRIDE_MODEL="claude-3-haiku-20240307"
DEFAULT_MODEL="claude-sonnet-4-6"
ANTHROPIC_PRESET="test_anthropic_preset"
ANTHROPIC_EFFORT="low"
GLOBAL_CONFIG="$HOME/forge/.forge.toml"
DEBUG_FILE="/tmp/forge-req-test.json"
PASS=0
FAIL=0

# ── helpers ──────────────────────────────────────────────────────────────────

log()  { printf '\n[TEST] %s\n' "$*"; }
ok()   { printf '  PASS: %s\n' "$*"; ((PASS++)) || true; }
fail() { printf '  FAIL: %s\n' "$*"; ((FAIL++)) || true; }

# Append test preset blocks to the global config and remove them on exit.
# presets is a HashMap<String, Preset> — cannot be set via env vars.
PRESET_BLOCK="
[presets.${ANTHROPIC_PRESET}]
reasoning = { enabled = true, effort = \"${ANTHROPIC_EFFORT}\" }
[presets.minimax_low]
reasoning = { enabled = true, effort = \"low\" }
[presets.minimax_medium]
reasoning = { enabled = true, effort = \"medium\" }
[presets.minimax_high]
reasoning = { enabled = true, effort = \"high\" }
"

append_presets() {
    printf '%s' "$PRESET_BLOCK" >> "$GLOBAL_CONFIG"
}

remove_presets() {
    perl -i -0pe "s/\Q${PRESET_BLOCK}\E//" "$GLOBAL_CONFIG"
    rm -f "$DEBUG_FILE"
}

trap remove_presets EXIT

check_field() {
    local label="$1"
    local file="$2"
    local python_expr="$3"   # evaluated against the parsed JSON dict `d`
    local expected="$4"

    if [[ ! -f "$file" ]]; then
        fail "$label: debug file not created ($file)"
        return
    fi

    local actual
    actual=$(python3 -c "import json; d=json.load(open('$file')); print($python_expr)")

    if [[ "$actual" == "$expected" ]]; then
        ok "$label: $actual"
    else
        fail "$label: expected '$expected', got '$actual'"
    fi
}

# ── sanity checks (no LLM call needed) ───────────────────────────────────────

log "Sanity check 1: FORGE_SESSION__MODEL_ID is parsed"
actual=$(FORGE_SESSION__MODEL_ID=test-sentinel "$BINARY" config get model 2>/dev/null)
if [[ "$actual" == "test-sentinel" ]]; then
    ok "session model override prints 'test-sentinel'"
else
    fail "expected 'test-sentinel', got '$actual'"
fi

log "Sanity check 2: FORGE_AGENT_FORGE__MODEL_ID does not break config parsing"
actual=$(FORGE_AGENT_FORGE__MODEL_ID=gpt-4o "$BINARY" config get model 2>/dev/null)
if [[ "$actual" == "$DEFAULT_MODEL" ]]; then
    ok "session model unchanged ('$actual')"
else
    fail "unexpected value '$actual'"
fi

log "Sanity check 3: all three agent overrides together do not break parsing"
actual=$(
    FORGE_AGENT_FORGE__MODEL_ID=gpt-4o \
    FORGE_AGENT_MUSE__MODEL_ID=claude-3-5-haiku-20241022 \
    FORGE_AGENT_SAGE__MODEL_ID=gpt-4o-mini \
        "$BINARY" config get model 2>/dev/null
)
if [[ "$actual" == "$DEFAULT_MODEL" ]]; then
    ok "session model unchanged with all three overrides set ('$actual')"
else
    fail "unexpected value '$actual'"
fi

# ── HTTP request body checks ──────────────────────────────────────────────────

log "agent_forge (FORGE_AGENT_FORGE__MODEL_ID): verify model in POST body"
rm -f "$DEBUG_FILE"
FORGE_DEBUG_REQUESTS="$DEBUG_FILE" \
FORGE_AGENT_FORGE__MODEL_ID="$OVERRIDE_MODEL" \
    "$BINARY" --agent forge -p "say hello" 2>/dev/null
check_field "agent_forge model" "$DEBUG_FILE" "d.get('model','<missing>')" "$OVERRIDE_MODEL"

log "agent_muse (FORGE_AGENT_MUSE__MODEL_ID): verify model in POST body"
rm -f "$DEBUG_FILE"
FORGE_DEBUG_REQUESTS="$DEBUG_FILE" \
FORGE_AGENT_MUSE__MODEL_ID="$OVERRIDE_MODEL" \
    "$BINARY" --agent muse -p "say hello" 2>/dev/null
check_field "agent_muse model" "$DEBUG_FILE" "d.get('model','<missing>')" "$OVERRIDE_MODEL"

log "agent_sage (FORGE_AGENT_SAGE__MODEL_ID): verify model in POST body"
rm -f "$DEBUG_FILE"
FORGE_DEBUG_REQUESTS="$DEBUG_FILE" \
FORGE_AGENT_SAGE__MODEL_ID="$OVERRIDE_MODEL" \
    "$BINARY" --agent sage -p "say hello" 2>/dev/null
check_field "agent_sage model" "$DEBUG_FILE" "d.get('model','<missing>')" "$OVERRIDE_MODEL"

# ── preset reasoning effort checks — Anthropic ───────────────────────────────
#
# Presets define reasoning.effort; the Anthropic provider serializes this as
# output_config.effort in the POST body (crates/forge_app/src/dto/anthropic/request.rs).
# FORGE_AGENT_*__PRESET_ID references the preset by name; the preset definition
# lives in ~/forge/.forge.toml (HashMap keys cannot be set via env vars).

append_presets

log "agent_forge preset (FORGE_AGENT_FORGE__PRESET_ID=$ANTHROPIC_PRESET): verify output_config.effort in POST body"
rm -f "$DEBUG_FILE"
FORGE_DEBUG_REQUESTS="$DEBUG_FILE" \
FORGE_AGENT_FORGE__PRESET_ID="$ANTHROPIC_PRESET" \
    "$BINARY" --agent forge -p "say hello" 2>/dev/null
check_field "agent_forge reasoning effort" "$DEBUG_FILE" \
    "d.get('output_config',{}).get('effort','<missing>')" "$ANTHROPIC_EFFORT"

log "agent_muse preset (FORGE_AGENT_MUSE__PRESET_ID=$ANTHROPIC_PRESET): verify output_config.effort in POST body"
rm -f "$DEBUG_FILE"
FORGE_DEBUG_REQUESTS="$DEBUG_FILE" \
FORGE_AGENT_MUSE__PRESET_ID="$ANTHROPIC_PRESET" \
    "$BINARY" --agent muse -p "say hello" 2>/dev/null
check_field "agent_muse reasoning effort" "$DEBUG_FILE" \
    "d.get('output_config',{}).get('effort','<missing>')" "$ANTHROPIC_EFFORT"

log "agent_sage preset (FORGE_AGENT_SAGE__PRESET_ID=$ANTHROPIC_PRESET): verify output_config.effort in POST body"
rm -f "$DEBUG_FILE"
FORGE_DEBUG_REQUESTS="$DEBUG_FILE" \
FORGE_AGENT_SAGE__PRESET_ID="$ANTHROPIC_PRESET" \
    "$BINARY" --agent sage -p "say hello" 2>/dev/null
check_field "agent_sage reasoning effort" "$DEBUG_FILE" \
    "d.get('output_config',{}).get('effort','<missing>')" "$ANTHROPIC_EFFORT"

# ── preset reasoning effort checks — sage via OpenRouter minimax/minimax-m2 ──
#
# For OpenRouter, reasoning effort is serialized as reasoning.effort in the POST
# body (crates/forge_app/src/dto/openai/request.rs). Three effort levels are
# tested to confirm each value is forwarded verbatim.

for effort in low medium high; do
    log "agent_sage · OpenRouter minimax/minimax-m2 · preset minimax_${effort}: verify reasoning.effort in POST body"
    rm -f "$DEBUG_FILE"
    FORGE_DEBUG_REQUESTS="$DEBUG_FILE" \
    FORGE_AGENT_SAGE__MODEL_ID="minimax/minimax-m2" \
    FORGE_AGENT_SAGE__PROVIDER_ID="open_router" \
    FORGE_AGENT_SAGE__PRESET_ID="minimax_${effort}" \
        "$BINARY" --agent sage -p "say hello" 2>/dev/null
    check_field "agent_sage minimax effort=${effort}" "$DEBUG_FILE" \
        "d.get('reasoning',{}).get('effort','<missing>')" "$effort"
done

# ── summary ───────────────────────────────────────────────────────────────────

printf '\n%s\n' "────────────────────────────────────"
printf 'Results: %d passed, %d failed\n' "$PASS" "$FAIL"

if [[ "$FAIL" -gt 0 ]]; then
    exit 1
fi
