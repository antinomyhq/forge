#!/bin/bash

# generate-description.sh - Generate PR description based on analysis
# Usage: ./generate-description.sh [--template=TYPE] [--output=FILE] [commit-range]

set -euo pipefail

# Default values
TEMPLATE="standard"
OUTPUT_FILE="/tmp/pr-description.md"
COMMIT_RANGE="main..HEAD"
ANALYSIS_FILE="/tmp/pr-analysis.md"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --template=*)
            TEMPLATE="${1#*=}"
            shift
            ;;
        --output=*)
            OUTPUT_FILE="${1#*=}"
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [--template=TYPE] [--output=FILE] [commit-range]"
            echo ""
            echo "Options:"
            echo "  --template=TYPE    Template type: standard, feature, bugfix, refactor, breaking, security, docs, large, mixed, auto"
            echo "  --output=FILE      Output file path (default: /tmp/pr-description.md)"
            echo "  commit-range       Git commit range (default: main..HEAD)"
            echo ""
            echo "Templates:"
            echo "  standard    General purpose template"
            echo "  feature     New functionality template"
            echo "  bugfix      Bug fix template with root cause analysis"
            echo "  refactor    Code improvement template"
            echo "  breaking    Breaking changes with migration guide"
            echo "  security    Security update template"
            echo "  docs        Documentation update template"
            echo "  large       Large PR template with progressive disclosure"
            echo "  mixed       Multiple orthogonal changes template"
            echo "  auto        Automatically detect appropriate template"
            echo ""
            echo "Examples:"
            echo "  $0                                    # Generate with standard template"
            echo "  $0 --template=auto                   # Auto-detect best template"
            echo "  $0 --template=large                  # Use large PR template"
            echo "  $0 --template=mixed --output=pr.md   # Generate mixed changes template"
            exit 0
            ;;
        *)
            COMMIT_RANGE="$1"
            shift
            ;;
    esac
done

echo "🚀 Generating PR description..."
echo "  Template: $TEMPLATE"
echo "  Output: $OUTPUT_FILE"
echo "  Range: $COMMIT_RANGE"
echo ""

# First, run analysis to gather information
SCRIPT_DIR="$(dirname "$0")"
if ! "$SCRIPT_DIR/analyze-changes.sh" "$COMMIT_RANGE" "$ANALYSIS_FILE"; then
    echo "❌ Failed to analyze changes"
    exit 1
fi

# Extract key information from analysis
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
COMMIT_COUNT=$(git rev-list --count "$COMMIT_RANGE")
FILES_CHANGED=$(git diff --name-only "$COMMIT_RANGE" | wc -l)

# Detect change type if not specified
if [ "$TEMPLATE" = "auto" ]; then
    # Read analysis data for better detection
    if [ -f "$ANALYSIS_FILE" ]; then
        TOTAL_FILES=$(grep "Files changed:" "$ANALYSIS_FILE" | grep -o '[0-9]*' || echo "1")
        CHANGE_AREAS_COUNT=$(grep "Change areas:" "$ANALYSIS_FILE" | grep -o '[0-9]*' || echo "1")
        
        # Check for large PR indicators
        if [ "$TOTAL_FILES" -gt 20 ]; then
            TEMPLATE="large"
        # Check for multiple orthogonal changes
        elif [ "$CHANGE_AREAS_COUNT" -gt 3 ]; then
            TEMPLATE="mixed"
        # Check specific patterns
        elif grep -q "BREAKING\|breaking" "$ANALYSIS_FILE"; then
            TEMPLATE="breaking"
        elif grep -q "feat\|feature" "$ANALYSIS_FILE"; then
            TEMPLATE="feature"
        elif grep -q "fix\|bug" "$ANALYSIS_FILE"; then
            TEMPLATE="bugfix"
        elif grep -q "refactor" "$ANALYSIS_FILE"; then
            TEMPLATE="refactor"
        elif grep -q "docs\|documentation" "$ANALYSIS_FILE"; then
            TEMPLATE="docs"
        elif grep -q "security\|vulnerability" "$ANALYSIS_FILE"; then
            TEMPLATE="security"
        else
            TEMPLATE="standard"
        fi
    else
        # Fallback to git-based detection
        if grep -q "BREAKING\|breaking" "$ANALYSIS_FILE"; then
            TEMPLATE="breaking"
        elif git log --grep="feat" $COMMIT_RANGE | head -1 | grep -q "feat"; then
            TEMPLATE="feature"
        elif git log --grep="fix" $COMMIT_RANGE | head -1 | grep -q "fix"; then
            TEMPLATE="bugfix"
        else
            TEMPLATE="standard"
        fi
    fi
    echo "🔍 Auto-detected template: $TEMPLATE"
fi

# Generate description based on template
cat > "$OUTPUT_FILE" << EOF
# Generated PR Description

**Branch**: $CURRENT_BRANCH  
**Commits**: $COMMIT_COUNT  
**Files Changed**: $FILES_CHANGED  
**Template**: $TEMPLATE  

---

EOF

case "$TEMPLATE" in
    "large")
        cat >> "$OUTPUT_FILE" << 'EOF'
# [PR Title] - Large Change Overview

## 🎯 High-Level Summary
**Primary Objective**: Brief 1-2 sentence summary of the main goal
**Scope**: What components/areas are affected
**Change Type**: Feature/Refactor/Migration/etc.

## 📋 Change Categories

<details>
<summary><strong>🔧 Core Changes</strong> (Click to expand)</summary>

### Component A: [Name]
**Purpose**: What this component change accomplishes
**Files**: List of key files
**Key Changes**:
- [ ] Specific change 1
- [ ] Specific change 2

### Component B: [Name]
**Purpose**: What this component change accomplishes
**Files**: List of key files
**Key Changes**:
- [ ] Specific change 1
- [ ] Specific change 2

</details>

<details>
<summary><strong>🧪 Testing & Validation</strong></summary>

### Test Coverage
- [ ] Unit tests added/updated
- [ ] Integration tests added/updated
- [ ] End-to-end tests verified

### Manual Testing
- [ ] Core functionality verified
- [ ] Edge cases tested
- [ ] Performance impact assessed

</details>

<details>
<summary><strong>📚 Supporting Changes</strong></summary>

### Documentation
- [ ] README updated
- [ ] API docs updated
- [ ] Code comments added

### Configuration
- [ ] Environment variables updated
- [ ] Config files modified
- [ ] Migration scripts added

</details>

## 🔍 Review Guidance

### Review Priority Order
1. **Start Here**: [Most critical files/components]
2. **Core Logic**: [Key implementation files]
3. **Supporting**: [Tests, docs, config]

### Focus Areas
- **Architecture**: [Specific architectural decisions to review]
- **Performance**: [Performance implications to consider]
- **Security**: [Security aspects to validate]

### Time Estimate
**Estimated Review Time**: [X hours - be realistic]
**Complexity Level**: High/Medium
**Recommended Approach**: Multiple review sessions

## ⚠️ Risks & Mitigation

### Identified Risks
- **Risk 1**: [Description] → **Mitigation**: [How it's addressed]
- **Risk 2**: [Description] → **Mitigation**: [How it's addressed]

### Deployment Strategy
- [ ] Gradual rollout planned
- [ ] Feature flags implemented
- [ ] Rollback procedure documented

## 📊 Impact Analysis

### Breaking Changes
- [ ] None
- [ ] API changes (see migration guide below)
- [ ] Database schema changes
- [ ] Configuration changes required

### Performance Impact
- **Expected**: [Description of expected impact]
- **Measured**: [Actual measurements if available]
- **Monitoring**: [What to watch post-deployment]

## 🚀 Migration Guide
[Include if there are breaking changes or required actions]

### For Developers
```bash
# Steps developers need to take
```

### For Deployment
```bash
# Steps for deployment
```

## 🔗 Related Work
- Closes #[issue-number]
- Related to #[issue-number]
- Follows up on #[pr-number]

## 📝 Notes for Reviewers
- **Context**: [Important background context]
- **Decisions**: [Key decisions made and why]
- **Future Work**: [What's planned for follow-up PRs]
EOF
        ;;

    "mixed")
        cat >> "$OUTPUT_FILE" << 'EOF'
## What
Mixed changes: [Brief summary of all change types included]

## 📋 Change Categories

### 🆕 New Features
- **Feature 1**: [Description]
  - Files: [Key files]
  - Purpose: [What it accomplishes]

### 🐛 Bug Fixes  
- **Fix 1**: [Description]
  - Issue: [What was broken]
  - Root Cause: [Why it was broken]
  - Solution: [How it's fixed]

### 🔧 Refactoring
- **Refactor 1**: [Description]
  - Goal: [What's being improved]
  - Scope: [What's included]

### 📚 Documentation
- **Doc Update 1**: [Description]
  - Reason: [Why it was needed]

### ⚙️ Configuration
- **Config Change 1**: [Description]
  - Impact: [Who/what is affected]

## Why Each Change is Included
**Bundling Rationale**: [Explain why these changes are together]
- [ ] Changes are interdependent
- [ ] Small changes bundled for efficiency
- [ ] Related to same feature/epic
- [ ] Emergency fixes included with planned work

## Testing Strategy

### Per Change Type
**Features**: [How new features were tested]
**Bug Fixes**: [How fixes were validated]
**Refactoring**: [How behavior preservation was verified]

### Integration Testing
- [ ] All changes work together
- [ ] No conflicts between changes
- [ ] Combined impact assessed

## Review Approach
1. **Review by change type** (features first, then fixes, etc.)
2. **Verify independence** of orthogonal changes
3. **Check interactions** between related changes

## Split Consideration
**Could this PR be split?** [Yes/No and explanation]
**If yes, suggested splits**:
- PR 1: [Changes that go together]
- PR 2: [Other changes that go together]
EOF
        ;;

    "feature")
        cat >> "$OUTPUT_FILE" << 'EOF'
## What
[Feature Name]: Brief description of the new functionality

## Why
**Problem**: Describe the user need or business requirement
**Impact**: How this improves user experience or system capabilities

## How
**Approach**: High-level implementation strategy
**Key Components**: Main parts of the solution

## Changes
### Core Features
- [ ] Feature implementation
- [ ] API endpoints (if applicable)
- [ ] Database changes (if applicable)

### Supporting Changes
- [ ] Tests added/updated
- [ ] Documentation updated
- [ ] Error handling added

## Testing
### Unit Tests
- [ ] Core functionality tested
- [ ] Edge cases covered

### Integration Tests
- [ ] API endpoints tested
- [ ] Database operations verified

### Manual Testing
- [ ] Happy path verified
- [ ] Error scenarios tested

## Screenshots/Demo
[Include screenshots, GIFs, or demo links if applicable]

## Migration Notes
[If applicable - database migrations, config changes, deployment steps]

## Rollback Plan
[How to revert if issues arise]
EOF
        ;;
    
    "bugfix")
        cat >> "$OUTPUT_FILE" << 'EOF'
## What
Fix for [Issue #XXX]: Brief description of the bug

## Problem
**Root Cause**: What was causing the issue
**Impact**: Who/what was affected
**Reproduction**: Steps to reproduce (if not obvious)

## Solution
**Approach**: How the fix works
**Changes**: Specific code/logic changes made

## Changes
### Bug Fix
- [ ] Core issue resolved
- [ ] Edge cases handled

### Prevention
- [ ] Tests added to prevent regression
- [ ] Input validation improved (if applicable)
- [ ] Error handling enhanced

## Testing
### Regression Tests
- [ ] Original bug scenario tested
- [ ] Related functionality verified

### Manual Verification
- [ ] Issue reproduction steps no longer reproduce
- [ ] Normal functionality unaffected

## Validation
- [ ] Issue reporter confirmed fix
- [ ] QA team verified (if applicable)
- [ ] Production-like environment tested

## Risk Assessment
**Low/Medium/High**: Brief justification
**Mitigation**: Steps to minimize risk
EOF
        ;;
    
    "refactor")
        cat >> "$OUTPUT_FILE" << 'EOF'
## What
Refactor [Component/Module]: Brief description of code improvements

## Why
**Technical Debt**: What issues this addresses
**Benefits**: Improved maintainability, performance, or readability
**Future Enablement**: How this supports future development

## How
**Strategy**: Approach taken for refactoring
**Scope**: What's included/excluded in this change

## Changes
### Code Structure
- [ ] File/module reorganization
- [ ] Function/method extraction
- [ ] Interface improvements

### Technical Improvements
- [ ] Performance optimizations
- [ ] Code duplication removal
- [ ] Type safety improvements

### Maintenance
- [ ] Documentation updates
- [ ] Comment improvements
- [ ] Dead code removal

## Testing
### Behavior Preservation
- [ ] All existing tests pass
- [ ] No functional changes verified
- [ ] Performance benchmarks maintained

### Code Quality
- [ ] Linting passes
- [ ] Code coverage maintained/improved
- [ ] Static analysis clean

## Notes
**No Functional Changes**: This is purely a refactoring - no user-facing behavior changes
**Review Focus**: Code structure, maintainability, and architectural improvements
EOF
        ;;
    
    "breaking")
        cat >> "$OUTPUT_FILE" << 'EOF'
## ⚠️ BREAKING CHANGES

## What
[Change Description]: Brief summary of breaking changes

## Breaking Changes
### API Changes
- **Removed**: List removed endpoints/methods
- **Changed**: List modified signatures/behavior
- **Added**: List new required parameters

### Configuration Changes
- **Environment Variables**: Changes to env vars
- **Config Files**: Changes to configuration format
- **Dependencies**: New required dependencies

## Migration Guide
### For API Consumers
```bash
# Old way
curl -X GET /api/old-endpoint

# New way
curl -X GET /api/new-endpoint -H "Version: 2.0"
```

### For Configuration
```yaml
# Old config
old_setting: value

# New config
new_section:
  setting: value
```

## Timeline
- **Deprecation Notice**: [Date when announced]
- **Breaking Change**: [Date when implemented]
- **Support End**: [Date when old version stops working]

## Backward Compatibility
- [ ] Migration script provided
- [ ] Documentation updated
- [ ] Support team notified
- [ ] Monitoring/alerting updated

## Rollback Plan
[Detailed steps to revert breaking changes if needed]

## Communication
- [ ] Breaking change announced to stakeholders
- [ ] Migration documentation published
- [ ] Support team trained on changes
EOF
        ;;
    
    "security")
        cat >> "$OUTPUT_FILE" << 'EOF'
## 🔒 Security Update

## What
Security improvement: Brief description of security changes

## Security Issues Addressed
- **Issue Type**: [Authentication, Authorization, Input Validation, etc.]
- **Severity**: [Critical/High/Medium/Low]
- **Impact**: Who/what is affected

## Changes
### Security Enhancements
- [ ] Authentication improvements
- [ ] Authorization fixes
- [ ] Input sanitization
- [ ] Data encryption

### Monitoring & Logging
- [ ] Security event logging added
- [ ] Audit trail improvements
- [ ] Alert mechanisms updated

## Testing
### Security Testing
- [ ] Penetration testing performed
- [ ] Vulnerability scan clean
- [ ] Security review completed

### Functional Testing
- [ ] Normal operations verified
- [ ] Performance impact assessed
- [ ] User experience maintained

## Deployment
### Security Considerations
- [ ] Gradual rollout planned
- [ ] Monitoring enhanced
- [ ] Incident response ready

## Notes
**Confidential**: Security details not included in public description
**Contact**: Security team for sensitive information
EOF
        ;;
    
    "docs")
        cat >> "$OUTPUT_FILE" << 'EOF'
## What
Documentation update: Brief description of what docs were changed

## Why
**Need**: Why this documentation update was necessary
**Audience**: Who this helps (developers, users, etc.)

## Changes
### Content Updates
- [ ] New sections added
- [ ] Existing content revised
- [ ] Examples updated/added
- [ ] Outdated information removed

### Structure Improvements
- [ ] Navigation improved
- [ ] Content reorganized
- [ ] Cross-references added
- [ ] Search optimization

## Quality Checks
- [ ] Technical accuracy verified
- [ ] Links tested and working
- [ ] Code examples validated
- [ ] Spelling/grammar reviewed

## Review Notes
**Focus Areas**: Specific sections that need careful review
**Target Audience**: Primary readers of this documentation
EOF
        ;;
    
    *)  # standard template
        cat >> "$OUTPUT_FILE" << 'EOF'
## What
Brief summary of what this PR accomplishes (1-2 sentences)

## Why
Context and motivation - what problem does this solve?

## How
High-level approach taken to implement the solution

## Changes
- List of key changes organized by component
- Focus on user-facing and significant internal changes
- Include any breaking changes or migration requirements

## Testing
- How changes were validated
- New tests added or existing tests modified
- Manual testing performed

## Notes
- Any important implementation details
- Future work or known limitations
- Review focus areas
EOF
        ;;
esac

# Add Forge attribution
cat >> "$OUTPUT_FILE" << 'EOF'

---

<sub>Generated using Forge Code</sub>
EOF

# Append analysis data for reference
cat >> "$OUTPUT_FILE" << EOF

---

## Analysis Data (Reference)
EOF

cat "$ANALYSIS_FILE" >> "$OUTPUT_FILE"

echo "✅ PR description generated successfully!"
echo ""
echo "📄 Output file: $OUTPUT_FILE"
echo ""
echo "Next steps:"
echo "1. Review and customize the generated description"
echo "2. Fill in placeholder sections with specific details"
echo "3. Remove analysis data section before publishing"
echo "4. Copy to your PR or create PR with: gh pr create --body-file $OUTPUT_FILE"
echo ""
echo "💡 Tip: Use 'cat $OUTPUT_FILE' to view the generated content"