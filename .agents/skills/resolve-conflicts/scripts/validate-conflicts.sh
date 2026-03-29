#!/bin/bash
# Validates that all Git conflicts have been resolved
# Returns 0 if no conflicts remain, 1 otherwise

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if we're in a git repository
if ! git rev-parse --git-dir > /dev/null 2>&1; then
    echo -e "${RED}Error: Not a git repository${NC}" >&2
    exit 1
fi

# Function to check for conflict markers in files
check_conflict_markers() {
    local files_with_markers=()
    
    # Search for conflict markers in tracked files
    while IFS= read -r file; do
        if [[ -f "$file" ]] && grep -l '^<<<<<<<\|^=======\|^>>>>>>>' "$file" > /dev/null 2>&1; then
            files_with_markers+=("$file")
        fi
    done < <(git diff --name-only --diff-filter=U 2>/dev/null || git ls-files)
    
    if [[ ${#files_with_markers[@]} -gt 0 ]]; then
        echo -e "${RED}✗ Found conflict markers in the following files:${NC}"
        printf '  %s\n' "${files_with_markers[@]}"
        return 1
    fi
    
    return 0
}

# Function to check for unmerged paths
check_unmerged_paths() {
    local unmerged_files
    unmerged_files=$(git diff --name-only --diff-filter=U 2>/dev/null || true)
    
    if [[ -n "$unmerged_files" ]]; then
        echo -e "${RED}✗ Found unmerged paths:${NC}"
        echo "$unmerged_files" | sed 's/^/  /'
        return 1
    fi
    
    return 0
}

# Function to check for both deleted and modified status
check_deleted_modified() {
    local status
    status=$(git status --porcelain 2>/dev/null || true)
    
    # Look for DU (deleted by us) or UD (deleted by them) or DD (both deleted) status
    local deleted_modified=$(echo "$status" | grep -E '^(DU|UD|DD|UA|AU|AA)' || true)
    
    if [[ -n "$deleted_modified" ]]; then
        echo -e "${YELLOW}⚠ Found files with delete/modify conflicts:${NC}"
        echo "$deleted_modified" | sed 's/^/  /'
        return 1
    fi
    
    return 0
}

# Function to check merge state
check_merge_state() {
    if git rev-parse MERGE_HEAD > /dev/null 2>&1; then
        echo -e "${YELLOW}⚠ Repository is still in merge state${NC}"
        echo "  Run 'git merge --continue' after resolving all conflicts"
        return 1
    fi
    
    if [[ -f .git/MERGE_HEAD ]]; then
        echo -e "${YELLOW}⚠ MERGE_HEAD file exists${NC}"
        return 1
    fi
    
    return 0
}

# Main validation
echo "🔍 Validating conflict resolution..."
echo ""

all_clear=true

if ! check_conflict_markers; then
    all_clear=false
fi

if ! check_unmerged_paths; then
    all_clear=false
fi

if ! check_deleted_modified; then
    all_clear=false
fi

if ! check_merge_state; then
    all_clear=false
fi

echo ""
if [[ "$all_clear" == true ]]; then
    echo -e "${GREEN}✓ All conflicts resolved successfully!${NC}"
    echo ""
    echo "Next steps:"
    echo "  1. Review changes: git diff --cached"
    echo "  2. Run tests to validate"
    echo "  3. Commit: git commit"
    exit 0
else
    echo -e "${RED}✗ Conflicts still exist. Please resolve them before continuing.${NC}"
    exit 1
fi
