#!/bin/bash

# Test script for semantic search quality validation
# This tests the validation logic without requiring actual LLM calls

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_DIR="/tmp/test_semantic_eval_$$"

echo "======================================"
echo "Testing Semantic Search Quality Eval"
echo "======================================"
echo

mkdir -p "$TEST_DIR"

# Test 1: Validation passes when sem_search is used
echo "Test 1: Check that sem_search tool detection works..."
cat > "$TEST_DIR/context.json" <<'EOF'
{
  "messages": [
    {
      "role": "assistant",
      "tool_calls": [
        {
          "id": "call_1",
          "type": "function",
          "function": {
            "name": "sem_search",
            "arguments": "{\"queries\":[{\"query\":\"test query\",\"use_case\":\"test case\"}]}"
          }
        }
      ]
    }
  ]
}
EOF

if cat "$TEST_DIR/context.json" | jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "sem_search")] | any' > /dev/null 2>&1; then
  echo "✓ Test 1 PASSED: sem_search tool detected correctly"
else
  echo "✗ Test 1 FAILED: sem_search tool not detected"
  exit 1
fi
echo

# Test 2: Validation skips when sem_search is NOT used
echo "Test 2: Check that missing sem_search is detected..."
cat > "$TEST_DIR/context2.json" <<'EOF'
{
  "messages": [
    {
      "role": "assistant",
      "tool_calls": [
        {
          "id": "call_1",
          "type": "function",
          "function": {
            "name": "fs_search",
            "arguments": "{\"pattern\":\"test\"}"
          }
        }
      ]
    }
  ]
}
EOF

if ! cat "$TEST_DIR/context2.json" | jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "sem_search")] | any' > /dev/null 2>&1; then
  echo "✓ Test 2 PASSED: Missing sem_search detected correctly"
else
  echo "✗ Test 2 FAILED: False positive for sem_search"
  exit 1
fi
echo

# Test 3: Check that llm_judge.ts can be loaded
echo "Test 3: Check that llm_judge.ts script loads..."
if npx tsx "$SCRIPT_DIR/llm_judge.ts" 2>&1 | grep -q "Missing required arguments"; then
  echo "✓ Test 3 PASSED: llm_judge.ts script loads correctly"
else
  echo "✗ Test 3 FAILED: llm_judge.ts script failed to load"
  exit 1
fi
echo

# Test 4: Check conditional execution logic
echo "Test 4: Test conditional LLM judge execution..."
cat > "$TEST_DIR/test_conditional.sh" <<'SCRIPT'
#!/bin/bash
if ! cat "$1" | jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "sem_search")] | any' > /dev/null 2>&1; then
  echo "Skipping LLM judge: sem_search tool was not used"
  exit 0
fi
echo "Would run LLM judge here"
exit 0
SCRIPT

chmod +x "$TEST_DIR/test_conditional.sh"

# Test with sem_search present
if output=$("$TEST_DIR/test_conditional.sh" "$TEST_DIR/context.json" 2>&1) && echo "$output" | grep -q "Would run LLM judge"; then
  echo "✓ Test 4a PASSED: Conditional runs LLM judge when sem_search present"
else
  echo "✗ Test 4a FAILED: Conditional logic incorrect"
  exit 1
fi

# Test with sem_search missing
if output=$("$TEST_DIR/test_conditional.sh" "$TEST_DIR/context2.json" 2>&1) && echo "$output" | grep -q "Skipping LLM judge"; then
  echo "✓ Test 4b PASSED: Conditional skips LLM judge when sem_search missing"
else
  echo "✗ Test 4b FAILED: Conditional logic incorrect"
  exit 1
fi
echo

# Cleanup
rm -rf "$TEST_DIR"

echo "======================================"
echo "All validation tests PASSED ✓"
echo "======================================"
echo
echo "Note: Full LLM judge testing requires Google Cloud authentication."
echo "To set up authentication:"
echo "  1. Run: gcloud auth application-default login"
echo "  2. Or set GOOGLE_APPLICATION_CREDENTIALS to service account JSON path"
