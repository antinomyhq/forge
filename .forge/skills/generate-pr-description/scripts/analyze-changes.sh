#!/bin/bash

# analyze-changes.sh - Analyze git changes and generate PR description context
# Usage: ./analyze-changes.sh [commit-range]

set -euo pipefail

# Default to comparing current branch with main
COMMIT_RANGE="${1:-main..HEAD}"
OUTPUT_FILE="${2:-/tmp/pr-analysis.md}"

echo "🔍 Analyzing changes in range: $COMMIT_RANGE"

# Create analysis file
cat > "$OUTPUT_FILE" << 'EOF'
# PR Analysis Report

## Branch Information
EOF

# Get current branch info
echo "- **Current Branch**: $(git rev-parse --abbrev-ref HEAD)" >> "$OUTPUT_FILE"
echo "- **Base Branch**: $(git merge-base HEAD main | git name-rev --name-only --stdin)" >> "$OUTPUT_FILE"
echo "- **Commit Count**: $(git rev-list --count $COMMIT_RANGE)" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

# Analyze commit messages
echo "## Commit Messages" >> "$OUTPUT_FILE"
echo '```' >> "$OUTPUT_FILE"
git log --oneline $COMMIT_RANGE >> "$OUTPUT_FILE"
echo '```' >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

# Enhanced file changes analysis with component detection
echo "## File Changes Analysis" >> "$OUTPUT_FILE"

# Detect components/modules based on directory structure
echo "### Component Impact" >> "$OUTPUT_FILE"
git diff --name-only $COMMIT_RANGE | sed 's|/[^/]*$||' | sort | uniq -c | sort -nr | head -10 | while read count dir; do
    if [ "$dir" != "." ]; then
        echo "- **$dir/**: $count files" >> "$OUTPUT_FILE"
    fi
done
echo "" >> "$OUTPUT_FILE"

# Detailed file changes with change type detection
echo "### Files Modified" >> "$OUTPUT_FILE"
git diff --name-status $COMMIT_RANGE | while read status file; do
    case $status in
        A) echo "- **Added**: \`$file\`" ;;
        M) echo "- **Modified**: \`$file\`" ;;
        D) echo "- **Deleted**: \`$file\`" ;;
        R*) echo "- **Renamed**: \`$file\`" ;;
        *) echo "- **$status**: \`$file\`" ;;
    esac
done >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

# Statistics
echo "### Change Statistics" >> "$OUTPUT_FILE"
STATS=$(git diff --shortstat $COMMIT_RANGE)
if [ -n "$STATS" ]; then
    echo "- $STATS" >> "$OUTPUT_FILE"
else
    echo "- No changes detected" >> "$OUTPUT_FILE"
fi
echo "" >> "$OUTPUT_FILE"

# Language/file type breakdown
echo "### File Types Changed" >> "$OUTPUT_FILE"
git diff --name-only $COMMIT_RANGE | sed 's/.*\.//' | sort | uniq -c | sort -nr | head -10 | while read count ext; do
    echo "- **.$ext**: $count files" >> "$OUTPUT_FILE"
done
echo "" >> "$OUTPUT_FILE"

# Enhanced change analysis with orthogonal detection
echo "## Change Analysis" >> "$OUTPUT_FILE"

# Detect orthogonal changes by analyzing different areas
echo "### Orthogonal Changes Detected" >> "$OUTPUT_FILE"

# Check for different change types
FEATURE_FILES=$(git diff --name-only $COMMIT_RANGE | grep -E "(feature|new)" || true)
BUGFIX_FILES=$(git diff --name-only $COMMIT_RANGE | grep -E "(fix|bug)" || true)
TEST_FILES=$(git diff --name-only $COMMIT_RANGE | grep -E "(test|spec)" || true)
DOC_FILES=$(git diff --name-only $COMMIT_RANGE | grep -E "(\.md$|README|CHANGELOG|doc)" || true)
CONFIG_FILES=$(git diff --name-only $COMMIT_RANGE | grep -E "(\.toml$|\.yaml$|\.yml$|\.json$|config)" || true)
MIGRATION_FILES=$(git diff --name-only $COMMIT_RANGE | grep -E "migration" || true)

# Detect frontend vs backend changes
FRONTEND_FILES=$(git diff --name-only $COMMIT_RANGE | grep -E "\.(js|jsx|ts|tsx|css|scss|html|vue)$" || true)
BACKEND_FILES=$(git diff --name-only $COMMIT_RANGE | grep -E "\.(rs|py|java|go|rb|php|cpp|c)$" || true)
DATABASE_FILES=$(git diff --name-only $COMMIT_RANGE | grep -E "(migration|schema|sql)" || true)

# Detect API changes
API_FILES=$(git diff --name-only $COMMIT_RANGE | grep -E "(api|endpoint|route|controller)" || true)

CHANGE_AREAS=()
[ -n "$FEATURE_FILES" ] && CHANGE_AREAS+=("**New Features**")
[ -n "$BUGFIX_FILES" ] && CHANGE_AREAS+=("**Bug Fixes**")
[ -n "$FRONTEND_FILES" ] && CHANGE_AREAS+=("**Frontend Changes**")
[ -n "$BACKEND_FILES" ] && CHANGE_AREAS+=("**Backend Changes**")
[ -n "$DATABASE_FILES" ] && CHANGE_AREAS+=("**Database Changes**")
[ -n "$API_FILES" ] && CHANGE_AREAS+=("**API Changes**")
[ -n "$TEST_FILES" ] && CHANGE_AREAS+=("**Test Changes**")
[ -n "$DOC_FILES" ] && CHANGE_AREAS+=("**Documentation**")
[ -n "$CONFIG_FILES" ] && CHANGE_AREAS+=("**Configuration**")

if [ ${#CHANGE_AREAS[@]} -gt 3 ]; then
    echo "⚠️ **Multiple orthogonal changes detected** - Consider splitting this PR:" >> "$OUTPUT_FILE"
    for area in "${CHANGE_AREAS[@]}"; do
        echo "- $area" >> "$OUTPUT_FILE"
    done
    echo "" >> "$OUTPUT_FILE"
    echo "💡 **Suggestion**: Split into separate PRs for easier review and safer deployment" >> "$OUTPUT_FILE"
elif [ ${#CHANGE_AREAS[@]} -gt 1 ]; then
    echo "📋 **Mixed change types detected**:" >> "$OUTPUT_FILE"
    for area in "${CHANGE_AREAS[@]}"; do
        echo "- $area" >> "$OUTPUT_FILE"
    done
else
    echo "✅ **Focused changes** - Single change area detected" >> "$OUTPUT_FILE"
fi
echo "" >> "$OUTPUT_FILE"

# Component-level analysis
echo "### Component-Level Impact" >> "$OUTPUT_FILE"
COMPONENTS=$(git diff --name-only $COMMIT_RANGE | cut -d'/' -f1-2 | sort | uniq)
COMPONENT_COUNT=$(echo "$COMPONENTS" | wc -l)

if [ "$COMPONENT_COUNT" -gt 5 ]; then
    echo "⚠️ **High component impact** ($COMPONENT_COUNT components affected)" >> "$OUTPUT_FILE"
    echo "**Components:**" >> "$OUTPUT_FILE"
    echo "$COMPONENTS" | head -10 | while read component; do
        if [ -n "$component" ]; then
            FILE_COUNT=$(git diff --name-only $COMMIT_RANGE | grep "^$component" | wc -l)
            echo "- \`$component/\` ($FILE_COUNT files)" >> "$OUTPUT_FILE"
        fi
    done
else
    echo "✅ **Moderate component impact** ($COMPONENT_COUNT components)" >> "$OUTPUT_FILE"
fi
echo "" >> "$OUTPUT_FILE"

# Complexity analysis
TOTAL_FILES=$(git diff --name-only $COMMIT_RANGE | wc -l)
TOTAL_LINES=$(git diff --numstat $COMMIT_RANGE | awk '{sum+=$1+$2} END {print sum}' || echo "0")

echo "### Complexity Assessment" >> "$OUTPUT_FILE"
if [ "$TOTAL_FILES" -gt 20 ] || [ "$TOTAL_LINES" -gt 500 ]; then
    echo "🔴 **Large PR detected**" >> "$OUTPUT_FILE"
    echo "- Files: $TOTAL_FILES (recommend <20)" >> "$OUTPUT_FILE"
    echo "- Lines changed: $TOTAL_LINES (recommend <500)" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"
    echo "**Recommendations for large PRs:**" >> "$OUTPUT_FILE"
    echo "1. Consider breaking into smaller, focused PRs" >> "$OUTPUT_FILE"
    echo "2. Use progressive review strategy (core changes first)" >> "$OUTPUT_FILE"
    echo "3. Provide detailed component breakdown in PR description" >> "$OUTPUT_FILE"
    echo "4. Schedule dedicated review time with team" >> "$OUTPUT_FILE"
elif [ "$TOTAL_FILES" -gt 10 ] || [ "$TOTAL_LINES" -gt 200 ]; then
    echo "🟡 **Medium PR** - manageable but consider structure" >> "$OUTPUT_FILE"
    echo "- Files: $TOTAL_FILES" >> "$OUTPUT_FILE"
    echo "- Lines changed: $TOTAL_LINES" >> "$OUTPUT_FILE"
else
    echo "🟢 **Small PR** - easy to review" >> "$OUTPUT_FILE"
    echo "- Files: $TOTAL_FILES" >> "$OUTPUT_FILE"
    echo "- Lines changed: $TOTAL_LINES" >> "$OUTPUT_FILE"
fi
echo "" >> "$OUTPUT_FILE"

# Pattern detection (existing logic enhanced)
echo "### Pattern Detection" >> "$OUTPUT_FILE"
[ -n "$TEST_FILES" ] && echo "- ✅ **Tests included**: Changes include test files" >> "$OUTPUT_FILE"
[ -n "$DOC_FILES" ] && echo "- 📚 **Documentation updated**: Changes include documentation" >> "$OUTPUT_FILE"
[ -n "$CONFIG_FILES" ] && echo "- ⚙️ **Configuration changes**: Changes include config files" >> "$OUTPUT_FILE"
[ -n "$MIGRATION_FILES" ] && echo "- 🗄️ **Database changes**: Changes include migrations" >> "$OUTPUT_FILE"

# Check for breaking changes indicators
BREAKING_PATTERNS="(BREAKING|breaking|removed|deprecated|changed.*signature)"
BREAKING_CHANGES=$(git log --grep="$BREAKING_PATTERNS" $COMMIT_RANGE || true)
if [ -n "$BREAKING_CHANGES" ]; then
    echo "- ⚠️ **Potential breaking changes detected**" >> "$OUTPUT_FILE"
fi
echo "" >> "$OUTPUT_FILE"

# Enhanced key code changes analysis
echo "## Detailed Code Analysis" >> "$OUTPUT_FILE"

# Function/method changes by component
echo "### Function/Method Changes by Component" >> "$OUTPUT_FILE"
for component in $(git diff --name-only $COMMIT_RANGE | cut -d'/' -f1 | sort | uniq | head -5); do
    if [ -n "$component" ] && [ "$component" != "." ]; then
        echo "#### $component/" >> "$OUTPUT_FILE"
        git diff $COMMIT_RANGE -- "$component" | grep -E "^(\+|\-).*(fn |function |def |class |impl )" | head -10 | while read line; do
            if [[ $line == +* ]]; then
                echo "- **Added**: \`${line:1}\`" >> "$OUTPUT_FILE"
            elif [[ $line == -* ]]; then
                echo "- **Removed**: \`${line:1}\`" >> "$OUTPUT_FILE"
            fi
        done 2>/dev/null || echo "- No significant function changes" >> "$OUTPUT_FILE"
        echo "" >> "$OUTPUT_FILE"
    fi
done

# Generate suggested change type and PR strategy
echo "## PR Strategy Recommendations" >> "$OUTPUT_FILE"

echo "### Suggested Change Type" >> "$OUTPUT_FILE"
if [ -n "$MIGRATION_FILES" ] || [ -n "$BREAKING_CHANGES" ]; then
    echo "**MAJOR** - Breaking changes or migrations detected" >> "$OUTPUT_FILE"
elif git log --grep="feat" $COMMIT_RANGE | head -1 | grep -q "feat"; then
    echo "**MINOR** - New features detected" >> "$OUTPUT_FILE"
else
    echo "**PATCH** - Bug fixes or improvements" >> "$OUTPUT_FILE"
fi
echo "" >> "$OUTPUT_FILE"

# PR size strategy
echo "### Recommended PR Structure" >> "$OUTPUT_FILE"
if [ "$TOTAL_FILES" -gt 20 ]; then
    echo "**Large PR Strategy:**" >> "$OUTPUT_FILE"
    echo "1. **High-Level Summary** - Focus on main objectives" >> "$OUTPUT_FILE"
    echo "2. **Component Breakdown** - Organize changes by logical areas" >> "$OUTPUT_FILE"
    echo "3. **Progressive Disclosure** - Use collapsible sections" >> "$OUTPUT_FILE"
    echo "4. **Review Guidance** - Specify review order and focus areas" >> "$OUTPUT_FILE"
elif [ "$TOTAL_FILES" -gt 10 ]; then
    echo "**Medium PR Strategy:**" >> "$OUTPUT_FILE"
    echo "1. **Clear Grouping** - Group related changes together" >> "$OUTPUT_FILE"
    echo "2. **Highlight Key Changes** - Focus on most important modifications" >> "$OUTPUT_FILE"
    echo "3. **Testing Strategy** - Comprehensive testing approach" >> "$OUTPUT_FILE"
else
    echo "**Standard PR Strategy:**" >> "$OUTPUT_FILE"
    echo "1. **Clear Purpose** - Single, focused objective" >> "$OUTPUT_FILE"
    echo "2. **Complete Context** - Full background and motivation" >> "$OUTPUT_FILE"
    echo "3. **Thorough Testing** - All scenarios covered" >> "$OUTPUT_FILE"
fi
echo "" >> "$OUTPUT_FILE"

# Related issues (existing logic)
echo "## Related Issues" >> "$OUTPUT_FILE"
git log --grep="closes\|fixes\|resolves" $COMMIT_RANGE | grep -oE "(closes|fixes|resolves) #[0-9]+" | sort | uniq >> "$OUTPUT_FILE" || echo "- No linked issues found" >> "$OUTPUT_FILE"

echo "✅ Analysis complete! Report saved to: $OUTPUT_FILE"
echo ""
echo "📋 Summary:"
echo "  - Files changed: $TOTAL_FILES"
echo "  - Lines changed: $TOTAL_LINES"
echo "  - Components affected: $COMPONENT_COUNT"
echo "  - Commits: $(git rev-list --count $COMMIT_RANGE)"
echo "  - Change areas: ${#CHANGE_AREAS[@]}"
echo "  - Tests included: $([ -n "$TEST_FILES" ] && echo "Yes" || echo "No")"
echo "  - Docs updated: $([ -n "$DOC_FILES" ] && echo "Yes" || echo "No")"

if [ ${#CHANGE_AREAS[@]} -gt 3 ]; then
    echo ""
    echo "⚠️  RECOMMENDATION: Consider splitting this PR - multiple orthogonal changes detected"
elif [ "$TOTAL_FILES" -gt 20 ]; then
    echo ""
    echo "⚠️  RECOMMENDATION: Large PR detected - use structured description template"
fi