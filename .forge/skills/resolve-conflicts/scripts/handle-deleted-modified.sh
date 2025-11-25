#!/bin/bash
# Handles deleted-but-modified file conflicts by creating backups and analyzing where changes should go
# Usage: ./handle-deleted-modified.sh

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

BACKUP_DIR=".git/conflict-backups/$(date +%Y%m%d-%H%M%S)"

# Check if we're in a git repository
if ! git rev-parse --git-dir > /dev/null 2>&1; then
    echo -e "${RED}Error: Not a git repository${NC}" >&2
    exit 1
fi

# Find files with delete/modify conflicts
find_deleted_modified() {
    git status --porcelain | grep -E '^(DU|UD|DD|UA|AU|AA)' || true
}

# Get the content from the branch that modified the file
get_modified_content() {
    local file="$1"
    local status="$2"
    
    case "$status" in
        DU) # Deleted by us, modified by them
            git show ":3:$file" 2>/dev/null || echo ""
            ;;
        UD) # Deleted by them, modified by us
            git show ":2:$file" 2>/dev/null || echo ""
            ;;
        *) 
            echo ""
            ;;
    esac
}

# Main processing
echo -e "${BLUE}🔍 Checking for deleted-but-modified files...${NC}"
echo ""

deleted_modified=$(find_deleted_modified)

if [[ -z "$deleted_modified" ]]; then
    echo -e "${GREEN}✓ No deleted-but-modified conflicts found${NC}"
    exit 0
fi

# Create backup directory
mkdir -p "$BACKUP_DIR"
echo -e "${YELLOW}Creating backups in: $BACKUP_DIR${NC}"
echo ""

# Process each conflicted file
while IFS= read -r line; do
    status="${line:0:2}"
    file="${line:3}"
    
    echo -e "${YELLOW}Processing: $file (status: $status)${NC}"
    
    # Get the modified content
    content=$(get_modified_content "$file" "$status")
    
    if [[ -n "$content" ]]; then
        # Create backup with directory structure
        backup_file="$BACKUP_DIR/$file"
        backup_dir=$(dirname "$backup_file")
        mkdir -p "$backup_dir"
        
        echo "$content" > "$backup_file"
        echo -e "  ${GREEN}✓${NC} Backed up to: $backup_file"
        
        # Try to find similar files (potential relocation targets)
        filename=$(basename "$file")
        base_name="${filename%.*}"
        extension="${filename##*.}"
        
        echo -e "  ${BLUE}Searching for potential relocation targets...${NC}"
        
        # Search for files with similar names
        similar_files=$(git ls-files | grep -i "$base_name" | grep -v "^$file$" || true)
        
        if [[ -n "$similar_files" ]]; then
            echo -e "  ${YELLOW}⚠ Potential relocation targets:${NC}"
            echo "$similar_files" | sed 's/^/    → /'
        else
            echo -e "  ${YELLOW}⚠ No obvious relocation target found${NC}"
            echo -e "  ${YELLOW}⚠ Changes may need to be manually integrated${NC}"
        fi
        
        # Create an analysis file
        analysis_file="$BACKUP_DIR/$file.analysis.txt"
        cat > "$analysis_file" << EOF
File: $file
Status: $status
Conflict Type: $([ "$status" = "DU" ] && echo "Deleted by us, modified by them" || echo "Deleted by them, modified by us")

Backed up to: $backup_file

Potential Actions:
1. If the file was renamed/moved: Apply changes to the new location
2. If the file was deleted intentionally: Review if changes are still needed
3. If the file was refactored: Distribute changes to new file structure

Potential Relocation Targets:
$similar_files

To view the changes:
  cat "$backup_file"

To compare with similar files:
$(echo "$similar_files" | while read -r sf; do echo "  diff \"$backup_file\" \"$sf\""; done)
EOF
        
        echo -e "  ${GREEN}✓${NC} Analysis saved to: $analysis_file"
    else
        echo -e "  ${RED}✗${NC} Could not retrieve content"
    fi
    
    # Resolve by removing (user must manually apply changes)
    if [[ "$status" == "DU" ]]; then
        git rm "$file" 2>/dev/null || true
        echo -e "  ${GREEN}✓${NC} Marked as deleted (ours)"
    elif [[ "$status" == "UD" ]]; then
        git add "$file" 2>/dev/null || git rm "$file" 2>/dev/null || true
        echo -e "  ${GREEN}✓${NC} Resolved conflict"
    fi
    
    echo ""
done <<< "$deleted_modified"

# Create a summary file
summary_file="$BACKUP_DIR/SUMMARY.md"
cat > "$summary_file" << EOF
# Conflict Resolution Summary

Generated: $(date)

## Deleted-Modified Files Processed

$(echo "$deleted_modified" | while IFS= read -r line; do
    status="${line:0:2}"
    file="${line:3}"
    echo "- **$file** (status: $status)"
done)

## Next Steps

1. Review each backup file in this directory
2. Identify where the changes should be applied
3. Manually integrate the changes into the appropriate files
4. Run tests to validate the integration
5. Commit the resolved changes

## Files Structure

$(find "$BACKUP_DIR" -type f -name "*.analysis.txt" | while read -r f; do
    file=$(basename "$f" .analysis.txt)
    echo "- \`$file\`"
    echo "  - Backup: \`$file\`"
    echo "  - Analysis: \`$file.analysis.txt\`"
done)

EOF

echo -e "${GREEN}✓ Summary created: $summary_file${NC}"
echo ""
echo -e "${BLUE}═══════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}✓ All deleted-but-modified files backed up successfully${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════${NC}"
echo ""
echo "Next steps:"
echo "  1. Review backups: ls -la $BACKUP_DIR"
echo "  2. Read summary: cat $summary_file"
echo "  3. Integrate changes manually into appropriate files"
echo "  4. Run validation: .forge/skills/resolve-conflicts/scripts/validate-conflicts.sh"
