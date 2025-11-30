---
name: generate-pr-description
description: Generate focused, professional pull request descriptions that clearly communicate changes and their impact. Use when creating new PRs, improving existing PR descriptions, or when asked to document code changes for review. Supports automatic analysis of git diffs, commit messages, and file changes to create comprehensive yet concise descriptions following best practices.
---

# Generate PR Description

Generate focused, professional pull request descriptions that effectively communicate the purpose, scope, and impact of code changes.

## Quick Start

```bash
# Analyze current changes and generate PR description
./scripts/analyze-changes.sh

# Generate description with auto-detection (recommended)
./scripts/generate-description.sh --template=auto

# Generate description for large PRs
./scripts/generate-description.sh --template=large

# Generate description for mixed changes
./scripts/generate-description.sh --template=mixed
```

## Core Workflow

1. **Analyze Changes**: Examine git diff, commit messages, and file modifications
2. **Detect Patterns**: Identify orthogonal changes, PR size, and complexity
3. **Select Template**: Choose appropriate template based on analysis
4. **Generate Description**: Create structured description with proper sections
5. **Validate Quality**: Ensure descriptions are focused, actionable, and complete

## Analysis Process

### 1. Gather Context
- Review git diff to understand code changes
- Analyze commit messages for intent and scope
- Identify modified files and their relationships
- Check for breaking changes, new features, or bug fixes
- **Detect orthogonal changes** across different components
- **Assess PR complexity** and size for appropriate structure

### 2. Extract Key Information
- **Purpose**: What problem does this solve?
- **Scope**: What components are affected?
- **Impact**: How does this change user experience or system behavior?
- **Dependencies**: Are there related changes or requirements?
- **Change Categories**: Separate independent changes for clarity

### 3. Generate Description
Use the appropriate template based on analysis:
- **Auto**: Automatically detect the best template
- **Large**: For PRs with >20 files or complex changes
- **Mixed**: For multiple orthogonal changes
- **Standard Templates**: Feature, bugfix, refactor, breaking, security, docs

## Templates and Structure

### Standard Template Structure
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

### Quality Guidelines

**Focus and Clarity**
- Lead with the most important information
- Use clear, actionable language
- Avoid technical jargon unless necessary
- Structure content for easy scanning

**Completeness**
- Include all information needed for effective review
- Explain the "why" behind significant decisions
- Document any trade-offs or limitations
- Provide context for reviewers unfamiliar with the area

**Brevity**
- Keep descriptions concise while maintaining completeness
- Use bullet points for better readability
- Avoid redundant information
- Focus on changes, not implementation details

## Advanced Features

### Custom Templates
Create domain-specific templates in `references/templates.md`:
- API changes template with endpoint documentation
- Database migration template with rollback procedures
- Frontend template with UI/UX impact details
- Security template with threat model considerations

### Integration with Tools
- **GitHub CLI**: Automatic PR creation with generated descriptions
- **Conventional Commits**: Extract change types and scopes
- **Issue Linking**: Automatically reference related issues
- **Team Workflows**: Adapt to team-specific review processes

## Bundled Resources

- **Scripts**: `scripts/analyze-changes.sh` and `scripts/generate-description.sh` - Automated git analysis and description generation
- **Templates**: `references/templates.md` - Proven PR description templates for different change types
- **Examples**: `references/examples.md` - Real-world examples of effective PR descriptions
- **Guidelines**: `references/guidelines.md` - Detailed best practices and team-specific standards

## When to Use Different Approaches

**Simple Changes** (1-5 files, clear purpose)
- Use `--template=auto` for automatic detection
- Focus on what changed and why

**Large Changes** (>20 files, multiple components)
- Use `--template=large` with progressive disclosure
- Include component breakdown and review guidance
- Break down changes by logical areas

**Mixed Changes** (multiple orthogonal changes)
- Use `--template=mixed` to separate change categories
- Explain why changes are bundled together
- Consider splitting into multiple PRs

**Breaking Changes** (API changes, migrations)
- Use `--template=breaking` with migration guide
- Include backward compatibility notes
- Document rollback procedures

**Emergency Fixes** (hotfixes, security patches)
- Use expedited template focusing on urgency
- Include validation and monitoring details