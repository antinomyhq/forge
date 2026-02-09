#!/bin/bash

set -e

CONTEXT_FILE="$1"
if [ -z "$CONTEXT_FILE" ]; then
  echo "Usage: $0 <context_file>"
  exit 1
fi

echo "=========================================="
echo "Running Semantic Search Quality Eval"
echo "=========================================="
echo ""

# Step 1: Check if sem_search was used
echo "Step 1: Checking for sem_search tool usage..."
if cat "$CONTEXT_FILE" | jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "sem_search")] | any' > /dev/null 2>&1; then
  echo "✓ sem_search tool found in context"
else
  echo "✗ FAILED: sem_search tool not used"
  exit 1
fi

# Step 2: Extract intent and queries
echo ""
echo "Step 2: Extracting task intent and queries..."
INTENT=$(cat "$CONTEXT_FILE" | jq -r '.task.intent // "implementation"')
echo "  Intent: $INTENT"

# Extract expected file types and avoid list from task
EXPECTED_TYPES=$(cat "$CONTEXT_FILE" | jq -r '.task.expected_file_types // [] | join(",")')
SHOULD_AVOID=$(cat "$CONTEXT_FILE" | jq -r '.task.should_avoid // [] | join(",")')

# Extract queries from tool calls
QUERIES=$(cat "$CONTEXT_FILE" | jq -r '[.messages[]?.tool_calls[]? | select(.function.name == "sem_search") | .function.arguments] | .[0]' 2>/dev/null || echo '{}')
echo "  Queries extracted: $(echo "$QUERIES" | jq -r '.queries | length // 0') query pairs"

# Step 3: Check authentication
echo ""
echo "Step 3: Checking Google Cloud authentication..."
if gcloud auth application-default print-access-token > /dev/null 2>&1; then
  echo "✓ Google Cloud authentication available"
  RUN_LLM_JUDGE=true
elif [ -n "$GOOGLE_APPLICATION_CREDENTIALS" ] && [ -f "$GOOGLE_APPLICATION_CREDENTIALS" ]; then
  echo "✓ Service account credentials found"
  RUN_LLM_JUDGE=true
else
  echo "⚠ Google Cloud authentication not configured"
  echo "  Skipping LLM judge evaluation"
  echo "  To enable: run 'gcloud auth application-default login'"
  RUN_LLM_JUDGE=false
fi

# Step 4: Run LLM judge (conditional)
echo ""
if [ "$RUN_LLM_JUDGE" = true ]; then
  echo "Step 4: Running LLM judge evaluation..."
  
  # Build command with parameters (use empty strings if not set)
  if [ -z "$EXPECTED_TYPES" ]; then
    EXPECTED_TYPES=""
  fi
  
  if [ -z "$SHOULD_AVOID" ]; then
    SHOULD_AVOID=""
  fi
  
  CMD="npx tsx llm_judge.ts --context \"$CONTEXT_FILE\" --intent \"$INTENT\" --expected-file-types \"$EXPECTED_TYPES\" --should-avoid \"$SHOULD_AVOID\""
  
  eval $CMD
  
  if [ $? -eq 0 ]; then
    echo ""
    echo "=========================================="
    echo "✓ EVALUATION PASSED"
    echo "=========================================="
    exit 0
  else
    echo ""
    echo "=========================================="
    echo "✗ EVALUATION FAILED"
    echo "=========================================="
    exit 1
  fi
else
  echo "Step 4: Skipped (authentication required)"
  echo ""
  echo "=========================================="
  echo "✓ VALIDATION PASSED (LLM judge skipped)"
  echo "=========================================="
  echo ""
  echo "Note: Full evaluation requires Google Cloud auth"
  exit 0
fi
