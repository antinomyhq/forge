---
name: create-pr-description
description: Generate and create pull request descriptions automatically using GitHub CLI. Use when the user asks to create a PR, generate a PR description, make a pull request, or submit changes for review. Analyzes git diff and commit history to create concise, meaningful PR descriptions that explain what changed and why.
---

# Create PR Description

Generate concise pull request descriptions and create PRs using GitHub CLI.

## Workflow

### 1. Verify Prerequisites

Check that there are changes to create a PR for:

```bash
# Get current branch
git branch --show-current

# Verify branch is not main/master
# Verify there are commits ahead of main
git log origin/main..HEAD --oneline
```

If on main/master or no commits ahead, inform the user there's nothing to create a PR for.

### 2. Analyze Changes

Gather context about the changes:

```bash
# Get commit messages
git log origin/main..HEAD --pretty=format:"%s"

# Get diff summary (files changed)
git diff origin/main..HEAD --stat

# Get actual code changes (sample key files if diff is large)
git diff origin/main..HEAD
```

**For large diffs**: Focus on the most meaningful changes. Sample key files rather than reading everything.

### 3. Determine Change Type

Classify the PR into one of these categories:

- **fix**: Bug fixes, error corrections, resolving issues
- **feature**: New functionality, capabilities, or enhancements
- **performance**: Speed improvements, optimization, efficiency gains
- **refactor**: Code restructuring without changing behavior

Base this on:
- Commit messages (keywords like "fix", "add", "optimize", "refactor")
- Nature of code changes (new files = feature, test fixes = fix, etc.)
- Scope of changes

### 4. Generate Description

Create a concise description with this structure:

```markdown
## [Change Type]: [One-line summary]

**Before**: [What was happening before - the problem, limitation, or state]

**After**: [What changed meaningfully - the solution, new capability, or improvement]

### Changes
- [High-level change 1]
- [High-level change 2]
- [High-level change 3]
```

**Guidelines**:
- Keep it concise - focus on high-level changes, not implementation details
- "Before" should explain the context or problem
- "After" should explain the meaningful impact
- Changes should be 3-5 bullet points maximum
- Use clear, direct language
- Don't include:
  - File-by-file breakdowns
  - Low-level implementation details
  - Boilerplate statements
  - Testing instructions (assumed)

**Examples**:

```markdown
## Feature: Add semantic code search

**Before**: No way to search codebase by concepts or behavior, only exact string matching.

**After**: Users can now search using natural language queries like "authentication flow" or "retry logic" to find relevant code across the repository.

### Changes
- Implemented semantic search using embeddings
- Integrated with existing search interface
- Added support for multiple concurrent queries
```

```markdown
## Fix: Resolve database connection timeout

**Before**: Service would hang indefinitely when database became unavailable, requiring manual restart.

**After**: Service now handles connection failures gracefully with automatic retry and timeout.

### Changes
- Added connection timeout configuration
- Implemented exponential backoff retry logic
- Improved error messages for connection failures
```

### 5. Create Pull Request

Use GitHub CLI to create the PR:

```bash
gh pr create --title "[Change Type]: [One-line summary]" --body "[Generated description]"
```

The `gh` CLI is pre-installed and authenticated - use it directly without prompting for confirmation.

### 6. Confirm

After creating the PR, provide the user with:
- PR URL
- Change type
- Brief summary of what was included

## Notes

- **Fully automated**: Don't prompt for additional input - analyze and create
- **Concise over comprehensive**: High-level impact, not exhaustive details
- **Context matters**: The before/after should tell a story of meaningful change
- **Trust the diff**: Let code changes guide the description, not assumptions
