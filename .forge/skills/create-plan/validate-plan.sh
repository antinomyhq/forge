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
    
    # Extract components for additional validation
    if [[ "$FILENAME" =~ ^([0-9]{4})-([0-9]{2})-([0-9]{2})-([a-z0-9-]+)-v([0-9]+)\.md$ ]]; then
        YEAR="${BASH_REMATCH[1]}"
        MONTH="${BASH_REMATCH[2]}"
        DAY="${BASH_REMATCH[3]}"
        TASK_NAME="${BASH_REMATCH[4]}"
        VERSION="${BASH_REMATCH[5]}"
        
        # 1a. Validate date is reasonable
        CURRENT_YEAR=$(date +%Y)
        if [ "$YEAR" -lt 2020 ] || [ "$YEAR" -gt $((CURRENT_YEAR + 1)) ]; then
            error "Year $YEAR seems unreasonable (should be between 2020 and $((CURRENT_YEAR + 1)))"
        fi
        
        if [ "$MONTH" -lt 1 ] || [ "$MONTH" -gt 12 ]; then
            error "Month $MONTH is invalid (must be 01-12)"
        fi
        
        if [ "$DAY" -lt 1 ] || [ "$DAY" -gt 31 ]; then
            error "Day $DAY is invalid (must be 01-31)"
        fi
        
        # 1b. Check task name is meaningful (not generic placeholders)
        GENERIC_NAMES="^(task|test|plan|temp|tmp|example|sample|demo|foo|bar)$"
        if [[ "$TASK_NAME" =~ $GENERIC_NAMES ]]; then
            warning "Task name '$TASK_NAME' is too generic. Use a descriptive name."
        fi
        
        # 1c. Check task name length (should be descriptive but not too long)
        TASK_NAME_LENGTH=${#TASK_NAME}
        if [ "$TASK_NAME_LENGTH" -lt 5 ]; then
            warning "Task name '$TASK_NAME' is very short. Consider a more descriptive name."
        elif [ "$TASK_NAME_LENGTH" -gt 60 ]; then
            warning "Task name is very long ($TASK_NAME_LENGTH chars). Consider shortening."
        fi
        
        # 1d. Check version number is reasonable
        if [ "$VERSION" -gt 50 ]; then
            warning "Version number $VERSION seems high. Are you sure this is correct?"
        fi
        
        # 1e. Check for uppercase letters or underscores (should use hyphens)
        if [[ "$FILENAME" =~ [A-Z_] ]]; then
            error "Filename contains uppercase letters or underscores. Use lowercase and hyphens only."
        fi
    fi
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
    error "Implementation Plan has only $CHECKBOX_COUNT tasks. Plans must have at least 3 tasks."
elif [ "$CHECKBOX_COUNT" -lt 5 ]; then
    warning "Implementation Plan has only $CHECKBOX_COUNT tasks. Consider adding more detailed steps."
elif [ "$CHECKBOX_COUNT" -gt 20 ]; then
    warning "Implementation Plan has $CHECKBOX_COUNT tasks. Consider grouping or creating sub-plans."
else
    success "Implementation Plan has $CHECKBOX_COUNT tasks"
fi

# 14. Check task quality and density
if [ "$CHECKBOX_COUNT" -gt 0 ]; then
    # Extract all task lines
    TASKS=$(echo "$CONTENT" | sed -n '/^## Implementation Plan$/,/^## /p' | grep --color=never -E '^\- \[ \]')
    
    # 14a. Check for very short tasks (< 20 chars after checkbox)
    SHORT_TASKS=""
    SHORT_COUNT=0
    while IFS= read -r task; do
        # Remove "- [ ] " prefix and numbering
        TASK_TEXT=$(echo "$task" | sed 's/^- \[ \] //' | sed 's/^[0-9]*\. *//')
        if [ ${#TASK_TEXT} -lt 20 ] && [ ${#TASK_TEXT} -gt 0 ]; then
            SHORT_TASKS="$SHORT_TASKS$TASK_TEXT"$'\n'
            SHORT_COUNT=$((SHORT_COUNT + 1))
        fi
    done <<< "$TASKS"
    
    if [ "$SHORT_COUNT" -gt 0 ]; then
        warning "Found $SHORT_COUNT task(s) with very short descriptions (< 20 chars). Tasks should be descriptive."
        echo "$SHORT_TASKS" | head -3 | sed 's/^/  - /'
    fi
    
    # 14b. Check for generic/vague task descriptions
    GENERIC_PATTERNS="(implement feature|add functionality|fix bug|update code|make changes|do work|complete task|finish|setup|configure)"
    GENERIC_TASKS=$(echo "$TASKS" | grep -iE "$GENERIC_PATTERNS" || true)
    if [ -n "$GENERIC_TASKS" ]; then
        warning "Found tasks with generic/vague descriptions. Be more specific about what needs to be done."
        echo "$GENERIC_TASKS" | head -3 | sed 's/^/  /'
    fi
    
    # 14c. Check average task length (should be descriptive)
    TOTAL_LENGTH=0
    TASK_COUNT=0
    while IFS= read -r task; do
        TASK_TEXT=$(echo "$task" | sed 's/^- \[ \] //' | sed 's/^[0-9]*\. *//')
        TASK_LEN=${#TASK_TEXT}
        TOTAL_LENGTH=$((TOTAL_LENGTH + TASK_LEN))
        TASK_COUNT=$((TASK_COUNT + 1))
    done <<< "$TASKS"
    
    if [ "$TASK_COUNT" -gt 0 ]; then
        AVG_LENGTH=$((TOTAL_LENGTH / TASK_COUNT))
        if [ "$AVG_LENGTH" -lt 30 ]; then
            warning "Average task description length is only $AVG_LENGTH characters. Tasks should be more detailed and include rationale."
        elif [ "$AVG_LENGTH" -gt 200 ]; then
            warning "Average task description length is $AVG_LENGTH characters. Consider breaking down complex tasks."
        else
            success "Task descriptions have good detail level (avg: $AVG_LENGTH chars)"
        fi
    fi
    
    # 14d. Check for potential duplicate or very similar tasks
    # Compare each task with others for similarity
    TASK_ARRAY=()
    while IFS= read -r task; do
        TASK_TEXT=$(echo "$task" | sed 's/^- \[ \] //' | sed 's/^[0-9]*\. *//' | tr '[:upper:]' '[:lower:]')
        TASK_ARRAY+=("$TASK_TEXT")
    done <<< "$TASKS"
    
    SIMILAR_FOUND=false
    for i in "${!TASK_ARRAY[@]}"; do
        for j in "${!TASK_ARRAY[@]}"; do
            if [ "$i" -lt "$j" ]; then
                TASK1="${TASK_ARRAY[$i]}"
                TASK2="${TASK_ARRAY[$j]}"
                # Check if tasks are very similar (same first 30 chars)
                TASK1_PREFIX="${TASK1:0:30}"
                TASK2_PREFIX="${TASK2:0:30}"
                if [ -n "$TASK1_PREFIX" ] && [ "$TASK1_PREFIX" = "$TASK2_PREFIX" ]; then
                    if [ "$SIMILAR_FOUND" = false ]; then
                        warning "Found potentially duplicate or very similar tasks. Review for redundancy."
                        SIMILAR_FOUND=true
                    fi
                fi
            fi
        done
    done
    
    # 14e. Check task numbering consistency
    NUMBERED_TASKS=$(echo "$TASKS" | grep --color=never -E '^\- \[ \] [0-9]+\.')
    if [ -n "$NUMBERED_TASKS" ]; then
        NUMBERED_COUNT=$(echo "$NUMBERED_TASKS" | wc -l | tr -d ' ')
        if [ "$NUMBERED_COUNT" -eq "$CHECKBOX_COUNT" ]; then
            # All tasks are numbered - check sequence
            NUMBERS=$(echo "$NUMBERED_TASKS" | sed 's/^- \[ \] \([0-9]*\)\..*/\1/')
            EXPECTED=1
            SEQUENCE_OK=true
            while IFS= read -r num; do
                if [ "$num" -ne "$EXPECTED" ]; then
                    SEQUENCE_OK=false
                    break
                fi
                EXPECTED=$((EXPECTED + 1))
            done <<< "$NUMBERS"
            
            if [ "$SEQUENCE_OK" = true ]; then
                success "Task numbering is consistent and sequential"
            else
                warning "Task numbering is inconsistent. Should be sequential: 1, 2, 3, ..."
            fi
        elif [ "$NUMBERED_COUNT" -gt 0 ]; then
            warning "Only $NUMBERED_COUNT of $CHECKBOX_COUNT tasks are numbered. Be consistent."
        fi
    fi
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
