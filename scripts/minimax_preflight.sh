#!/usr/bin/env bash
#
# MiniMax Preflight Check for Forge
# Tests connectivity to MiniMax API endpoints (both OpenAI-compatible and Anthropic-compatible)
#
# Usage:
#   export MINIMAX_API_KEY="your-api-key"
#   ./scripts/minimax_preflight.sh

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

die() {
  echo -e "${RED}error: $*${NC}" >&2
  exit 2
}

warn() {
  echo -e "${YELLOW}warning: $*${NC}" >&2
}

success() {
  echo -e "${GREEN}ok: $*${NC}" >&2
}

# Get API key from environment
key="${MINIMAX_API_KEY:-}"
if [[ -z "${key}" ]]; then
  die "no API key set. Export MINIMAX_API_KEY with your MiniMax API key."
fi

# Create temp directory for responses
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

# API endpoints
CHAT_URL="https://api.minimax.io/v1/chat/completions"
MSG_URL="https://api.minimax.io/anthropic/v1/messages"

# Test payloads
PAYLOAD_CHAT='{"model":"MiniMax-M2.5","messages":[{"role":"user","content":"ping"}],"max_tokens":16,"stream":false}'
PAYLOAD_MSG='{"model":"MiniMax-M2.5","max_tokens":16,"messages":[{"role":"user","content":"ping"}]}'

echo ""
echo "========================================"
echo "  MiniMax API Preflight Check"
echo "========================================"
echo ""

# Test OpenAI-compatible endpoint
echo "Checking OpenAI-compatible endpoint (/v1/chat/completions)..."
CODE_CHAT="$(
  curl -sS --max-time 20 \
    -o "${tmp}/chat.json" \
    -w "%{http_code}" \
    -H "Authorization: Bearer ${key}" \
    -H "Content-Type: application/json" \
    -H "Accept-Language: en-US,en" \
    "${CHAT_URL}" \
    -d "${PAYLOAD_CHAT}" \
    || true
)"

if [[ "${CODE_CHAT}" == "200" ]]; then
  success "OpenAI-compatible endpoint reachable (HTTP 200)"
  exit 0
fi

# Parse error response
python3 - <<PY 2>/dev/null || true
import json, sys
p = "${tmp}/chat.json"
try:
  data = json.load(open(p))
except Exception:
  print(f"warn: chat error body (first 240): {open(p,'rb').read(240)!r}")
  sys.exit(0)
msg = data.get("error", {}).get("message") or data.get("message") or str(data)[:240]
print(f"warn: chat http=${CODE_CHAT} message={msg[:240]}")
PY

echo ""

# Test Anthropic-compatible endpoint
echo "Checking Anthropic-compatible endpoint (/anthropic/v1/messages)..."
CODE_MSG="$(
  curl -sS --max-time 20 \
    -o "${tmp}/msg.json" \
    -w "%{http_code}" \
    -H "Authorization: Bearer ${key}" \
    -H "anthropic-version: 2023-06-01" \
    -H "Content-Type: application/json" \
    "${MSG_URL}" \
    -d "${PAYLOAD_MSG}" \
    || true
)"

if [[ "${CODE_MSG}" == "200" ]]; then
  success "Anthropic-compatible endpoint reachable (HTTP 200)"
  exit 0
fi

# Parse error response
python3 - <<PY 2>/dev/null || true
import json, sys
p = "${tmp}/msg.json"
try:
  data = json.load(open(p))
except Exception:
  print(f"warn: messages error body (first 240): {open(p,'rb').read(240)!r}")
  sys.exit(0)
err = data.get("error", {})
msg = err.get("message") or data.get("message") or str(data)[:240]
print(f"warn: messages http=${CODE_MSG} message={msg[:240]}")
PY

echo ""
echo "========================================"
echo "  Preflight Summary"
echo "========================================"
echo ""
echo "OpenAI-compatible endpoint:      HTTP ${CODE_CHAT}"
echo "Anthropic-compatible endpoint:   HTTP ${CODE_MSG}"
echo ""

die "MiniMax preflight failed. Check your API key, account status, and base URLs."
