# PR Description Templates

## Standard Template

```markdown
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
```

## Feature Template

```markdown
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
```

## Bugfix Template

```markdown
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
```

## Refactor Template

```markdown
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
```

## Breaking Changes Template

```markdown
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
```

## Documentation Template

```markdown
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
```

## Security Template

```markdown
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
```

## Template Selection Guide

| Change Type | Use Template | Key Characteristics |
|-------------|-------------|-------------------|
| New feature or enhancement | Feature | Adds functionality, user-facing changes |
| Bug fix or issue resolution | Bugfix | Fixes existing problem, restores expected behavior |
| Code improvement without behavior change | Refactor | Internal changes, no user-facing impact |
| API changes, migrations | Breaking Changes | Requires user action, compatibility impact |
| README, guides, comments | Documentation | Information and guidance updates |
| Vulnerability fixes, auth changes | Security | Security-related improvements |

## Customization Guidelines

### Team-Specific Adaptations
- Add required sections for your team's workflow
- Include mandatory checklists (security, compliance, etc.)
- Adapt language and tone to match team culture
- Include links to team resources and tools

### Project-Specific Sections
- **Frontend**: Include UI/UX impact, browser compatibility
- **Backend**: Include API versioning, database impact
- **DevOps**: Include infrastructure changes, deployment notes
- **Mobile**: Include platform-specific considerations

### Integration Points
- **Issue Tracking**: Link to Jira, GitHub Issues, etc.
- **CI/CD**: Include build/deployment requirements
- **Monitoring**: Include metrics and alerting updates
- **Documentation**: Link to updated docs, runbooks, etc.

## Large PR Template

```markdown
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
```

## Mixed Changes Template

```markdown
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
```