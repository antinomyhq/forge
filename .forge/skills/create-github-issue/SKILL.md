---
name: create-github-issue
description: Create GitHub issues using GitHub CLI with support for templates, labels, assignees, milestones, and draft issues. Use when the user asks to create a GitHub issue, file a bug report, submit a feature request, or open an issue in a GitHub repository.
---

# Create GitHub Issue

Create comprehensive GitHub issues using `gh issue create` with proper formatting and organization.

## Workflow

### 1. Determine Issue Type

Classify the issue into one of these categories (maps to `type:` labels):

- **bug** → `type: bug` - Unexpected behavior, errors, or crashes
- **feature** → `type: feature` - New functionality or capabilities
- **enhancement** → `type: fix` - Improvements to existing functionality
- **documentation** → `type: docs` - Documentation-related issues
- **performance** → `type: performance` - Performance issues or optimizations
- **security** → `type: security` - Security vulnerabilities or concerns
- **refactoring** → `type: refactor` - Code refactoring or technical debt
- **chore** → `type: chore` - Routine tasks, maintenance, dependencies
- **testing** → `type: testing` - Test-related issues
- **discussion** → `type: discussion` - Questions, proposals, feedback

Base this on:
- User's description (keywords like "crash", "add", "slow", "confusing")
- Nature of the problem (unexpected behavior = bug, missing capability = feature)
- Impact on users

### 2. Gather Context (If Needed)

For bugs or complex issues, gather relevant information:

```bash
# Check current git status for context
git status

# View recent commits if related to codebase changes
git log --oneline -10

# Check if related issues exist


gh issue list --search "keyword" --limit 10
```

### 3. Select Template and Generate Body

Choose the appropriate template from [templates.md](references/templates.md) and fill in the details:

**Bug Report Template Structure:**
```markdown
## Description
[Brief description of the bug]

## Steps to Reproduce
1. Step one
2. Step two
3. Step three

## Expected Behavior
[What you expected to happen]

## Actual Behavior
[What actually happened]

## Environment
- OS: [e.g., macOS, Linux, Windows]
- Version: [e.g., v1.2.3]
- Configuration: [any relevant configuration]

## Logs/Error Messages
```
[Include relevant logs or error messages here]
```

## Additional Context
[Any other relevant information]
```

**Feature Request Template Structure:**
```markdown
## Summary
[Brief summary of the feature request]

## Problem Statement
[What problem does this feature solve? What is the current pain point?]

## Proposed Solution
[Describe the solution you'd like to see implemented]

## Alternatives Considered
[Describe any alternative solutions you've considered]

## Additional Context
[Any other relevant information, mockups, or examples]
```

**Enhancement Template Structure:**
```markdown
## Summary
[Brief summary of the enhancement]

## Current Behavior
[Describe the current behavior or implementation]

## Proposed Enhancement
[Describe the enhancement you'd like to see]

## Benefits
[What benefits would this enhancement provide?]

## Additional Context
[Any other relevant information]
```

**Documentation Request Template Structure:**
```markdown
## Summary
[Brief summary of the documentation request]

## What Needs Documentation
[Describe what documentation needs to be added or improved]

## Proposed Content
[Describe the content that should be included]

## Target Audience
[Who is the target audience for this documentation?]

## Additional Context
[Any other relevant information or examples]
```

**Performance Issue Template Structure:**
```markdown
## Summary
[Brief summary of the performance issue]

## Current Performance
[Describe the current performance characteristics]

## Expected Performance
[What performance improvement is expected?]

## Reproduction Steps
[Steps to reproduce the performance issue]

## Environment
- Hardware: [CPU, RAM, etc.]
- OS: [e.g., macOS, Linux, Windows]
- Version: [e.g., v1.2.3]
- Configurationgy: [any relevant configuration]

## Benchmarks/Measurements
[Include any benchmark results or measurements]

## Additional Context
[Any other relevant information]
```

**Security Issue Template Structure:**
```markdown
**IMPORTANT**: For security vulnerabilities, please follow the project's security policy or use a private channel to report.

## Summary
[Brief summary of the security issue]

## Vulnerability Description
[Describe the security vulnerability in detail]

## Impact
[What is the impact of this vulnerability?]

## Proof of Concept
[Include a proof of concept if available]

## Proposed Fix
[Describe how the vulnerability can be fixed]

## Additional Context
[Any other relevant information]
```

**Refactoring Request Template Structure:**
```markdown
## Summary
[Brief summary of the refactoring request]

## Current Implementation
[Describe the current implementation]

## Proposed Refactoring
[Describe the refactoring you'd like to see]

## Benefits
[What benefits would this refactoring provide? (e.g., maintainability, readability, performance)]

## Breaking Changes
[Are there any breaking changes? How should they be handled?]

## Additional Context
[Any other relevant information]
```

### 4. Choose Labels

Select appropriate labels from `.github/labels.json`. **Only use labels defined in that file.**

**Always include:**
- **Type label** (required): `type: bug`, `type: feature`, `type: fix`, `type: docs`, `type: performance`, `type: security`, `type: refactor`, `type: chore`, `type: testing`, `type: discussion`

**Optional:**
- **State label** (for status): `state: pending`, `state: blocked`, `state: approved`, `state: inactive`
- **Work label** (for complexity): `work: obvious`, `work: complicated`, `work: complex`, `work: chaotic`
- **Community label**: `good first issue`, `help`
- **Release label**: `release: breaking`, `release: skip changelog`
- **CI label**: `ci: lint`, `ci: benchmark`, `ci: test-jit`, `ci: build all targets`

**Note:** Read `.github/labels.json` to see the complete list of labels, their descriptions, and aliases.

### 5. Create Title

Write a clear, descriptive title following these guidelines:

- **Be concise**: Keep under 70 characters
- **Be descriptive**: Include the issue type and main component
- **Use imperative mood**: "Fix authentication bug" not "Authentication bug needs fixing"
- **Start with action verb**: Fix, Add, Improve, Update, Refactor, Document

Good titles:
- "Fix authentication timeout on login"
- "Add support for OAuth2"
- "Improve database query performance"
- "Update API documentation for v2"
- "Refactor user service for better testability"

Bad titles:
- "Problem with login"
- "It doesn't work"
- "Need to add OAuth"
- "Bug"
- "Feature request"

### 6. Create Issue

**Step 1: Write body to temp file**
```bash
# Write the generated body to .forge/FORGE_ISSUE_BODY.md
```

Use the `write` tool to create `.forge/FORGE_ISSUE_BODY.md` with the generated body content.

**Step 2: Create issue using the temp file**
```bash
.forge/skills/create-github-issue/scripts/create_issue.sh \
  --title "[Issue Title]" \
  --body "$(cat .forge/FORGE_ISSUE_BODY.md)" \
  --label "type: bug,work: complicated,state: pending"
```

**Optional parameters:**
```bash
# Assign to a specific user
--assignee "username"

# Add to a milestone
--milestone "milestone-number"

# Create as draft issue
--draft
```

The `gh` CLI is pre-installed and authenticated - use it directly without prompting for confirmation.

**Note:** The temp file `.forge/FORGE_ISSUE_BODY.md` should be deleted after issue creation. It's in `.forge/` directory which is typically gitignored.

### 7. Confirm

After creating the issue, provide the user with:
- Issue URL
- Issue type
- Brief summary of what was included

```bash
gh issue list --limit 1
```

## Issue Examples

### Example 1: Bug Report

```markdown
## Description
Users experience timeout errors when logging in with OAuth2 after approximately 30 seconds.

## Steps to Reproduce
1. Navigate to login page
2. Click 'Login with GitHub'
3. Wait for authentication to complete

## Expected Behavior
User should be logged in successfully.

## Actual Behavior
Authentication times out after 30 seconds with error: "Authentication timeout exceeded".

## Environment
- OS: macOS 14.0
- Version: v2.1.0
- Browser: Chrome 120
- Node.js: v18.17.0

## Logs/Error Messages
```
Error: Authentication timeout exceeded
    at OAuth2Strategy.authenticate (lib/auth/oauth2.ts:45)
    at processTicksAndRejections (node:internal/process/task_queues:96:5)
```

## Additional Context
This issue occurs intermittently, affecting approximately 10% of login attempts. The issue started appearing after upgrading to v2.1.0.
```

**Labels:** `type: bug,work: complicated,state: pending`

### Example 2: Feature Request

```markdown
## Summary
Add support for dark mode theme to improve accessibility and user experience.

## Problem Statement
Users have requested dark mode support through multiple feedback channels. The current light-only并发主题导致长时间使用时眼部疲劳，且不尊重用户的系统偏好设置。

## Proposed Solution
Implement a theme toggle with light/dark mode options using CSS variables. Auto-detect system theme preference as the default, with manual override available.

## Alternatives Considered
- Auto-detect system theme only (no manual override)
- Always dark mode
- Third-party theming library

## Additional Context
See attached mockups for the proposed UI. This feature was requested in issues #123 and #456.
```

**Labels:** `type: feature,work: complex,state: pending`

### Example 3: Performance Issue

```markdown
## Summary
Database query performance degrades significantly when processing large datasets (>100,000 records).

## Current Performance
Query takes 15-20 seconds for 100,000 records, causing API timeouts. CPU usage spikes to 100% during query execution.

## Expected Performance
Query should complete in under 2 seconds for 100,000 records.

## Reproduction Steps
1. Insert 100,000 records into the database
2. Execute: `GET /api/users?limit=100000`
3. Observe response time

## Environment
- Hardware: 4 CPU cores, 16GB RAM
- OS: Ubuntu 22.04 LTS
- Database: PostgreSQL 15.2
- Version: v3.1.0

## Benchmarks/Measurements
- 1,000 records: 0.2s
- 10,000 records: 2.1s
- 100,000 records: 18.5s
- 1,000,000 records: Timeout (>30s)

## Additional Context
Profiling shows the bottleneck is in the `ORDER BY created_at` clause. Adding an index might help, but needs investigation.
```

**Labels:** `type: performance,work: complicated,state: pending`

### Example 4: Documentation Request

```markdown
## Summary
API documentation is incomplete and missing examples for several endpoints.

## What Needs Documentation
- Authentication flow with OAuth2
- Rate limiting details
- Error response formats
- Pagination for list endpoints

## Proposed Content
Add comprehensive documentation including:
- Code examples in Python and JavaScript
- Request/response examples
- Common use cases
- Error handling best practices

## Target Audience
External developers integrating with our API.

## Additional Context
Currently, developers frequently open support tickets asking about these undocumented features. This has been mentioned in issues #789 and #890.
```

**Labels:** `type: docs,work: obvious,state: pending`

### Example 5: Refactoring Request

```markdown
## Summary
Refactor authentication module to use clean architecture patterns, improving testability and reducing coupling.

## Current Implementation
Authentication module has tight coupling between business logic and infrastructure (database, external OAuth providers). Business logic is spread across multiple files with unclear responsibilities.

## Proposed Refactoring
- Separate business logic from infrastructure dependencies
- Introduce repository pattern for data access
- Add service layer for authentication operations
- Extract interfaces for better mocking in tests

## Benefits
- Easier to test without external dependencies
- Clearer separation of concerns
- Simpler to add new authentication providers
- Reduced technical debt

## Breaking Changes
No breaking changes to public API. Internal refactoring only.

## Additional Context
This was identified in technical debt review #234. Current test coverage is 45% due to tight coupling.
```

**Labels:** `type: refactor,work: complex,state: pending`

### Example 6: Security Issue

```markdown
**IMPORTANT**: For security vulnerabilities, please follow the project's security policy or use a private channel to report.

## Summary
SQL injection vulnerability in user search endpoint allows arbitrary SQL execution.

## Vulnerability Description
The `GET /api/users/search` endpoint does not properly sanitize user input before constructing SQL queries. An attacker can inject malicious SQL code through the `q` parameter.

## Impact
An authenticated user can:
- Read any data in the database
- Modify or delete user records
- Access sensitive information (passwords, tokens)

## Proof of Concept
```bash
curl -H "Authorization: Bearer <token>" \
  "https://api.example.com/api/users/search?q=test%27%20OR%20%271%27%3D%271"
```
This returns all users instead of just matching ones.

## Proposed Fix
- Use parameterized queries instead of string concatenation
- Implement input validation and sanitization
- Add SQL injection tests to test suite

## Additional Context
This affects all versions since v1.0.0. Discovered during security audit.
```

**Labels:** `type: security,work: complex,state: blocked`

### Example 7: Enhancement (Simple)

```markdown
## Summary
Add keyboard shortcuts for common actions in the admin panel.

## Current Behavior
Users must click buttons to perform common actions (save, delete, edit), which is slower for power users.

## Proposed Enhancement
Add keyboard shortcuts:
- Cmd/Ctrl+S: Save
- Cmd/Ctrl+D: Delete
- Cmd/Ctrl+E: Edit
- Escape: Cancel

## Benefits
- Faster workflow for power users
- Improved accessibility
- Consistent with common web application patterns

## Additional Context
Requested by multiple users in feedback form.
```

**Labels:** `type: fix,work: obvious,state: pending`

## Draft Issues

Use `--draft` flag when:

- Issue needs more research or validation
- Proposal needs team discussion
- Not ready for community visibility
- Collecting information before formal submission
- Exploratory or speculative proposals

Example draft issue:

```markdown
## Summary
Proposal: Migrate to new database schema for improved performance.

## Current Schema
Current schema uses a monolithic table structure with 50+ columns, causing performance issues and making schema changes difficult.

## Proposed Schema
Normalized schema with separate tables for related entities:
- users (core user data)
- user_profiles (extended profile info)
- user_preferences (settings)
- user_metadata (key-value pairs)

## Migration Plan
1. Create new tables
2. Migrate existing data
3. Update application code
4. Run parallel validation
5. Switch over to new schema
6. Drop old tables

## Risks
- Data migration may fail for edge cases
- Performance impact during migration
- Application downtime during switchover
- Potential data loss if migration fails

## Questions for Discussion
- Should we use a gradual migration approach?
- How to handle rollback if issues arise?
- What's the acceptable downtime?
```

**Labels:** `type: refactor,work: complex,state: pending` with `--draft` flag

## Guidelines

### Essential Elements

Every issue must include:

- **Title**: Clear, descriptive, under 70 characters
- **Type label** (required): `type: bug`, `type: feature`, `type: fix`, `type: docs`, `type: performance`, `type: security`, `type: refactor`, `type: chore`, `type: testing`, `type: discussion`
- **Body**: Complete template filled with relevant information

### Optional but Recommended

- **State label**: `state: pending`, `state: blocked`, `state: approved`, `state: inactive`
- **Work label**: `work: obvious`, `work: complicated`, `work: complex`, `work: chaotic`
- **Community label**: `good first issue`, `help`
- **Release label**: `release: breaking`, `release: skip changelog`
- **Environment details**: For bugs and performance issues
- **Steps to reproduce**: For bugs
- **Links**: Related issues, documentation, or references

### What to Avoid

- Empty bodies or just a title
- Placeholder text like "TODO: fill this in"
- Vague titles like "Bug" or "Feature request"
- Missing type label
- Insufficient information to reproduce or understand the issue
- Personal checklists as the main content

### Anti-Patterns

❌ **Bad Issue Example:**
```markdown
## Bug
It doesn't work.
```
**Labels:** `type: bug`

✅ **Good Issue Example:**
```markdown
## Description
Users experience timeout errors when logging in with OAuth2.

## Steps to Reproduce
1. Navigate to login page
2. Click 'Login with GitHub'
3. Wait for authentication to complete

## Expected Behavior
User should be logged in successfully.

## Actual Behavior
Authentication times out after 30 seconds.

## Environment
- OS: macOS 14.0
- Version: v2.1.0
```
**Labels:** `type: bug,work: complicated,state: pending`

### When to Keep It Simple

For very small, obvious issues (typo fixes, trivial enhancements), you can use a shorter structure:
- Title
- Summary
- Context (brief)

But never skip the essential information needed to understand the issue.

### When to Be Comprehensive

- Bugs with complex reproduction steps
- New features or major functionality
- Performance issues with benchmarks
- Security vulnerabilities
- Breaking changes or deprecations
- Issues affecting multiple parts of the codebase

## Notes

**Key Principles:**
- **Be specific**: Provide concrete details, not vague descriptions
- **Include context**: Explain why this issue matters
- **Provide evidence**: Include logs, screenshots, or benchmarks when available
- **Use templates**: Follow the appropriate template structure
- **Label appropriately**: Help with triage and planning
- **Think about the reviewer**: Make it easy for others to understand and prioritize

**Label Strategy:**
- Always include one `type:` label (required)
- Include `state:` label for status tracking (pending, blocked, approved, inactive)
- Include `work:` label for complexity estimation (obvious, complicated, complex, chaotic)
- Use `good first issue` or `help` for community engagement
- Use `release:` labels for release information
- Use `ci:` labels for CI-specific tasks
- **Only use labels defined in `.github/labels.json`**