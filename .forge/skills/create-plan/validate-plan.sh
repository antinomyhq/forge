#!/usr/bin/env bash
# Validates the structure and content of a plan file
# Usage: ./validate-plan.sh <path-to-plan.md>

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Counters
ERRORS=0
WARNINGS=0

error() {
    echo -e "${RED}✗ ERROR:${NC} $1" >&2
    ((ERRORS+=1))
}

warning() {
    echo -e "${YELLOW}⚠ WARNING:${NC} $1" >&2
    ((WARNINGS+=1))
}

success() {
    echo -e "${GREEN}✓${NC} $1"
}

info() {
    echo "ℹ $1"
}

# Check if file path is provided
if [ $# -eq 0 ]; then
    echo "Usage: $0 <path-to-plan.md>"
    exit 1
fi

PLAN_FILE="$1"

# Check if file exists
if [ ! -f "$PLAN_FILE" ]; then
    error "File not found: $PLAN_FILE"
    exit 1
fi

info "Validating plan: $PLAN_FILE"
echo ""

# 1. Check file naming convention
FILENAME=$(basename "$PLAN_FILE")
if [[ ! "$FILENAME" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}-[a-z0-9-]+-v[0-9]+\.md$ ]]; then
    error "Filename must follow pattern: YYYY-MM-DD-task-name-vN.md (got: $FILENAME)"
else
    success "Filename follows naming convention"
fi

# 2. Check file is in plans directory
if [[ ! "$PLAN_FILE" =~ plans/ ]]; then
    warning "Plan should be in 'plans/' directory"
else
    success "Plan is in 'plans/' directory"
fi

# 3. Check required sections exist
CONTENT=$(cat "$PLAN_FILE")

required_sections=(
    "^# .+"
    "^## Objective"
    "^## Implementation Plan"
    "^## Verification Criteria"
    "^## Potential Risks and Mitigations"
    "^## Alternative Approaches"
)

section_names=(
    "Main heading (# Title)"
    "Objective section"
    "Implementation Plan section"
    "Verification Criteria section"
    "Potential Risks and Mitigations section"
    "Alternative Approaches section"
)

for i in "${!required_sections[@]}"; do
    if echo "$CONTENT" | grep -qE "${required_sections[$i]}"; then
        success "${section_names[$i]} present"
    else
        error "Missing required section: ${section_names[$i]}"
    fi
done

# 4. Check for markdown checkboxes in Implementation Plan
if echo "$CONTENT" | sed -n '/^## Implementation Plan$/,/^## /p' | grep -qE '^\- \[ \]'; then
    success "Implementation Plan uses checkbox format"
else
    error "Implementation Plan must use checkbox format: - [ ] Task description"
fi

# 5. Check for numbered lists in Implementation Plan (should not exist)
if echo "$CONTENT" | sed -n '/^## Implementation Plan$/,/^## /p' | grep -qE '^[0-9]+\.'; then
    error "Implementation Plan should NOT use numbered lists (1., 2., 3.). Use checkboxes instead: - [ ]"
fi

# 6. Check for plain bullet points in Implementation Plan (should not exist)
IMPL_SECTION=$(echo "$CONTENT" | sed -n '/^## Implementation Plan$/,/^## /p')
if echo "$IMPL_SECTION" | grep -E '^\- [^\[]' | grep -qv '^\- \[ \]'; then
    error "Implementation Plan should NOT use plain bullet points (-). Use checkboxes instead: - [ ]"
fi

# 7. Check for code blocks (should not exist)
CODE_FENCE='```'
if echo "$CONTENT" | grep -q "$CODE_FENCE"; then
    error "Plan contains code blocks. Plans should NEVER include code, only natural language descriptions"
else
    success "No code blocks found"
fi

# 8. Check for suspicious code patterns (excluding valid references)
# Allow: `filepath:line` references, markdown formatting, tool names
# Disallow: code-like patterns with semicolons, braces, function calls
SUSPICIOUS_CODE=$(echo "$CONTENT" | grep -E '`[^`]*[{};()].*[{};()][^`]*`' | grep -v -E '`[a-zA-Z0-9_/.-]+:[0-9-]+`' || true)
if [ -n "$SUSPICIOUS_CODE" ]; then
    warning "Potential code snippets detected (should use natural language instead):"
    echo "$SUSPICIOUS_CODE" | head -3
fi

# 9. Check that checkboxes have meaningful content (not placeholders)
PLACEHOLDER_TASKS=$(echo "$CONTENT" | grep -E '^\- \[ \] (\[.*\]|TODO|TBD|\.\.\.|\.\.\.)' || true)
if [ -n "$PLACEHOLDER_TASKS" ]; then
    warning "Found placeholder or template-style checkbox tasks:"
    echo "$PLACEHOLDER_TASKS"
fi

# 10. Check for empty sections
if echo "$CONTENT" | sed -n '/^## Objective$/,/^## /p' | grep -qE '^$' | grep -qE '^## '; then
    warning "Objective section appears to be empty"
fi

# 11. Check that verification criteria are specific (not empty)
VERIFICATION_CONTENT=$(echo "$CONTENT" | sed -n '/^## Verification Criteria$/,/^## /p' | tail -n +2 | grep -E '^\-' || true)
if [ -z "$VERIFICATION_CONTENT" ]; then
    error "Verification Criteria section must contain specific, measurable criteria"
else
    success "Verification Criteria section has content"
fi

# 12. Check that risks have mitigations
RISKS_SECTION=$(echo "$CONTENT" | sed -n '/^## Potential Risks and Mitigations$/,/^## /p')
if echo "$RISKS_SECTION" | grep -qE '^[0-9]+\.|^\*\*'; then
    if echo "$RISKS_SECTION" | grep -qi "mitigation"; then
        success "Risks section includes mitigations"
    else
        warning "Risks section should include mitigation strategies"
    fi
fi

# 13. Check minimum number of checkboxes (at least 3 tasks)
CHECKBOX_COUNT=$(echo "$CONTENT" | grep -cE '^\- \[ \]' || true)
if [ -z "$CHECKBOX_COUNT" ]; then
    CHECKBOX_COUNT=0
fi
if [ "$CHECKBOX_COUNT" -lt 3 ]; then
    warning "Implementation Plan has only $CHECKBOX_COUNT tasks. Consider breaking down into more specific steps."
elif [ "$CHECKBOX_COUNT" -gt 20 ]; then
    warning "Implementation Plan has $CHECKBOX_COUNT tasks. Consider grouping or creating sub-plans."
else
    success "Implementation Plan has $CHECKBOX_COUNT tasks"
fi

# Final summary
echo ""
echo "================================================"
if [ $ERRORS -eq 0 ]; then
    echo -e "${GREEN}✓ Validation passed${NC}"
    if [ $WARNINGS -gt 0 ]; then
        echo -e "${YELLOW}  ($WARNINGS warnings)${NC}"
    fi
    exit 0
else
    echo -e "${RED}✗ Validation failed${NC}"
    echo -e "  ${RED}$ERRORS errors${NC}, ${YELLOW}$WARNINGS warnings${NC}"
    exit 1
fi
