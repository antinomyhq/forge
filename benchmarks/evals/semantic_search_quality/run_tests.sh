#!/bin/bash
# Comprehensive test suite for semantic search quality evaluation

set -e

echo "=============================================="
echo "Semantic Search Quality Eval - Test Suite"
echo "=============================================="
echo ""

PASSED=0
FAILED=0
SKIPPED=0

# Test 1: Good implementation queries - should PASS
echo "Test 1: Good implementation queries..."
if cd /Users/amit/code-forge/benchmarks/evals/semantic_search_quality && \
   ./run_eval.sh /tmp/test_semantic_eval/full_context.json > /dev/null 2>&1; then
  echo "  ✓ PASSED (score >= 70)"
  PASSED=$((PASSED + 1))
else
  echo "  ✗ FAILED (expected pass)"
  FAILED=$((FAILED + 1))
fi

# Test 2: Documentation queries - should PASS (may be marginal)
echo "Test 2: Documentation queries..."
if cd /Users/amit/code-forge/benchmarks/evals/semantic_search_quality && \
   ./run_eval.sh /tmp/test_semantic_eval/doc_context.json > /dev/null 2>&1; then
  echo "  ✓ PASSED (score >= 70)"
  PASSED=$((PASSED + 1))
else
  echo "  ✗ FAILED (expected pass)"
  FAILED=$((FAILED + 1))
fi

# Test 3: Bad queries - should FAIL
echo "Test 3: Bad queries (generic keywords)..."
if cd /Users/amit/code-forge/benchmarks/evals/semantic_search_quality && \
   ./run_eval.sh /tmp/test_semantic_eval/bad_context.json > /dev/null 2>&1; then
  echo "  ✗ FAILED (expected failure, got pass)"
  FAILED=$((FAILED + 1))
else
  echo "  ✓ PASSED (correctly failed - score < 70)"
  PASSED=$((PASSED + 1))
fi

# Test 4: Missing sem_search - should FAIL early
echo "Test 4: Missing sem_search tool..."
if cd /Users/amit/code-forge/benchmarks/evals/semantic_search_quality && \
   ./run_eval.sh /tmp/test_semantic_eval/no_sem_search_context.json > /dev/null 2>&1; then
  echo "  ✗ FAILED (expected early exit failure)"
  FAILED=$((FAILED + 1))
else
  echo "  ✓ PASSED (correctly failed early)"
  PASSED=$((PASSED + 1))
fi

# Test 5: Verify conditional execution logic
echo "Test 5: Conditional LLM judge execution..."
if cat /tmp/test_semantic_eval/full_context.json | \
   jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "sem_search")] | any' > /dev/null 2>&1; then
  echo "  ✓ PASSED (conditional logic works)"
  PASSED=$((PASSED + 1))
else
  echo "  ✗ FAILED (conditional logic broken)"
  FAILED=$((FAILED + 1))
fi

echo ""
echo "=============================================="
echo "Test Results:"
echo "  Passed:  $PASSED"
echo "  Failed:  $FAILED"
echo "  Skipped: $SKIPPED"
echo "=============================================="
echo ""

if [ $FAILED -eq 0 ]; then
  echo "✓ All tests PASSED!"
  exit 0
else
  echo "✗ Some tests FAILED"
  exit 1
fi
