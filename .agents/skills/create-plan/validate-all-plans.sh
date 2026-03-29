#!/usr/bin/env bash
# Validates all plan files in the plans directory
# Usage: ./validate-all-plans.sh [plans-directory]

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VALIDATOR="$SCRIPT_DIR/validate-plan.sh"

# Check if validator exists
if [ ! -f "$VALIDATOR" ]; then
    echo -e "${RED}Error:${NC} Validator script not found at $VALIDATOR"
    exit 1
fi

# Make validator executable
chmod +x "$VALIDATOR"

# Get plans directory (default to plans/ in project root)
PLANS_DIR="${1:-plans}"

if [ ! -d "$PLANS_DIR" ]; then
    echo -e "${RED}Error:${NC} Plans directory not found: $PLANS_DIR"
    exit 1
fi

# Find all plan files
PLAN_FILES=$(find "$PLANS_DIR" -name "*.md" -type f | sort)

if [ -z "$PLAN_FILES" ]; then
    echo -e "${YELLOW}No plan files found in $PLANS_DIR${NC}"
    exit 0
fi

# Count files
TOTAL_FILES=$(echo "$PLAN_FILES" | wc -l | tr -d ' ')
PASSED=0
FAILED=0

echo -e "${BLUE}Validating $TOTAL_FILES plan file(s) in $PLANS_DIR${NC}"
echo ""

# Validate each file
while IFS= read -r plan_file; do
    echo -e "${BLUE}═══════════════════════════════════════════════${NC}"
    if "$VALIDATOR" "$plan_file"; then
        ((PASSED++))
    else
        ((FAILED++))
    fi
    echo ""
done <<< "$PLAN_FILES"

# Final summary
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${BLUE}Summary:${NC}"
echo -e "  Total:  $TOTAL_FILES"
echo -e "  ${GREEN}Passed: $PASSED${NC}"
echo -e "  ${RED}Failed: $FAILED${NC}"
echo ""

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}✓ All plans validated successfully!${NC}"
    exit 0
else
    echo -e "${RED}✗ Some plans failed validation${NC}"
    exit 1
fi
