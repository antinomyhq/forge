---
name: create-pr-description
description: Generate and create pull request descriptions automatically using GitHub CLI. Use when the user asks to create a PR, generate a PR description, make a pull request, or submit changes for review. Analyzes git diff and commit history to create comprehensive, meaningful PR descriptions that explain what changed, why it matters, and how to test it.
---

# Create PR Description

Generate comprehensive pull request descriptions and create PRs using GitHub CLI.

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
- **docs**: Documentation changes
- **test**: Test additions or improvements
- **chore**: Maintenance tasks, dependencies, configuration

Base this on:
- Commit messages (keywords like "fix", "add", "optimize", "refactor")
- Nature of code changes (new files = feature, test fixes = fix, etc.)
- Scope of changes

### 4. Generate Description

Create a comprehensive description with this structure:

```markdown
## Summary
[One sentence explaining what this PR does and why it matters]

## Context
[Background information, related issues, previous work, or the problem being solved]

## Changes
[High-level description of what changed]

### Key Implementation Details
[Technical details that help reviewers understand the approach, especially for complex changes]

## Use Cases
[Concrete examples of how this will be used - helps reviewers understand practical value]

## Testing
[How to test the changes - step-by-step instructions]

## Links
- Related issues: #123, #456
- Documentation: URL (if applicable)
- Original implementation: URL (if applicable)
```

### Description Guidelines

**Essential Elements:**
- **Summary**: One clear sentence explaining the change and its value
- **Context**: Why this change was needed, what problem it solves
- **Changes**: What was actually changed at a high level
- **Testing**: How reviewers can verify the changes

**Optional but Recommended:**
- **Implementation Details**: For complex changes, explain the technical approach
- **Use Cases**: Concrete examples of how the feature will be used
- **Links**: Related issues, documentation, papers, or original implementations
- **Known Issues**: Any limitations or known problems

**What to Avoid:**
- Empty descriptions or just issue links
- Placeholder text like "Fixes #(issue)"
- File-by-file breakdowns (unless necessary)
- Low-level implementation details (keep it high-level)
- Boilerplate statements
- Personal checklists as the main description

### Description Examples

**Example 1: Feature Addition**

```markdown
## Summary
Add semantic code search to enable searching codebase by concepts and behavior rather than exact string matching.

## Context
Currently, users can only search using exact string matching, which makes it difficult to find code based on functionality or behavior. This has been a recurring request in issues #123 and #456.

## Changes
- Implemented semantic search using vector embeddings
- Integrated with existing search interface
- Added support for multiple concurrent queries with result aggregation
- Configurable search scope (entire codebase or specific directories)

### Key Implementation Details
Uses OpenAI embeddings for code representation and cosine similarity for matching. Index is built incrementally to support large codebases. Search results are reranked based on code context and usage patterns.

## Use Cases
- Find authentication flow without knowing exact function names
- Locate retry logic across the codebase
- Search for "database connection" patterns

## Testing
```bash
# Run the search service
npm run search:dev

# Test semantic queries
curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{"query": "user authentication"}'
```

## Links
- Related issues: #123, #456
- Documentation: /docs/semantic-search.md
```

**Example 2: Bug Fix**

```markdown
## Summary
Fix database connection timeout that caused service to hang indefinitely when database became unavailable.

## Context
Service would hang indefinitely when database became unavailable, requiring manual restart. This was reported in production incident #789 and affected multiple users.

## Changes
- Added configurable connection timeout (default: 30 seconds)
- Implemented exponential backoff retry logic (max 5 retries)
- Improved error messages with specific failure reasons
- Added circuit breaker pattern to prevent cascading failures

### Key Implementation Details
Timeout is applied at the connection pool level. Backoff strategy: 1s, 2s, 4s, 8s, 16s. Circuit breaker opens after 5 consecutive failures and resets after 60 seconds.

## Testing
```bash
# Simulate database failure
docker-compose stop db

# Verify timeout and retry behavior
npm test -- tests/integration/connection-timeout.test.ts

# Verify circuit breaker activation
curl http://localhost:3000/health # Should return 503 after circuit opens
```

## Links
- Related issues: #789, #890
- Incident report: /incidents/2024-01-15-db-timeout.md
```

**Example 3: Performance Improvement**

```markdown
## Summary
Optimize image processing pipeline to reduce memory usage by 60% and improve throughput by 2.5x.

## Context
Current image processing implementation loads entire images into memory, causing OOM errors with large files and limiting throughput. This was identified as a performance bottleneck in profiling session #123.

## Changes
- Implemented streaming image processing using chunked reading
- Added parallel processing for multiple images
- Optimized memory allocation with object pooling
- Added caching for frequently accessed image metadata

### Key Implementation Details
Uses Node.js streams for memory-efficient processing. Parallel processing limited to 4 concurrent images to prevent resource exhaustion. Object pool reduces GC pressure by reusing buffers.

## Use Cases
- Process large images (>100MB) without OOM errors
- Batch process thousands of images efficiently
- Reduced memory footprint allows higher concurrent user load

## Testing
```bash
# Run performance benchmarks
npm run benchmark

# Test with large files
node tests/performance/large-files.test.js

# Verify memory usage
node --inspect tests/memory-usage.js
```

## Links
- Related issues: #456
- Performance report: /docs/performance/2024-01-image-processing.md
```

**Example 4: Refactor**

```markdown
## Summary
Refactor authentication module to use clean architecture patterns, improving testability and reducing coupling.

## Context
Authentication module had tight coupling between business logic and infrastructure, making it difficult to test and modify. This was identified in technical debt review #234.

## Changes
- Separated business logic from infrastructure dependencies
- Introduced repository pattern for data access
- Added service layer for authentication operations
- Extracted interfaces for better mocking in tests

### Key Implementation Details
Business logic now depends on interfaces rather than concrete implementations. Infrastructure (database, cache) is injected as dependencies. All services are unit-testable without external dependencies.

## Use Cases
- Easier to add new authentication providers (OAuth, SAML)
- Simpler to mock for unit tests
- Clear separation of concerns improves maintainability

## Testing
```bash
# Unit tests (no database required)
npm test tests/unit/auth/

# Integration tests (with real database)
npm test tests/integration/auth/

# Verify all existing functionality still works
npm run e2e
```

## Links
- Related issues: #234
- Architecture doc: /docs/architecture/auth-module.md
```

**Example 5: Simple Fix (Minimal but Complete)**

```markdown
## Summary
Fix typo in user welcome email template that caused incorrect company name to display.

## Context
Users were seeing "Welcome to [Wrong Company]" instead of the correct company name. Reported in #567.

## Changes
- Corrected company name in email template
- Added test to catch similar typos in the future

## Testing
```bash
# Run email template tests
npm test tests/unit/email-templates.test.ts

# Verify email renders correctly
npm run test:email --template=welcome
```

## Links
- Related issues: #567
```

### 5. Create Pull Request

Write the description to a temporary file and use GitHub CLI to create the PR:

**Step 1: Write description to temp file**
```bash
# Write the generated description to .forge/FORGE_PR_DESCRIPTION.md
```

Use the `write` tool to create `.forge/FORGE_PR_DESCRIPTION.md` with the generated description content.

**Step 2: Create PR using the temp file**
```bash
gh pr create --title "[Change Type]: [One-line summary]" --body-file .forge/FORGE_PR_DESCRIPTION.md
```

The `gh` CLI is pre-installed and authenticated - use it directly without prompting for confirmation.

**Note:** The temp file `.forge/FORGE_PR_DESCRIPTION.md` can not be left in place and should be deleted after PR creation. It's in `.forge/` directory which is typically gitignored.

### 6. Confirm

After creating the PR, provide the user with:
- PR URL
- Change type
- Brief summary of what was included

## Notes

**Key Principles:**
- **Context matters**: Explain why the change was made, not just what changed
- **Use cases help**: Concrete examples make abstract changes understandable
- **Testing is essential**: Always include how to verify the changes
- **Links provide depth**: Reference issues, docs, and implementations for context
- **Be honest**: Mention known issues or limitations
- **Respect reviewers' time**: A good description reduces review effort

**Anti-Patterns to Avoid:**
- Empty descriptions or just issue links
- Placeholder text like "Fixes #(issue)"
- File-by-file breakdowns (unless necessary)
- Personal checklists as the main description
- Assuming reviewers know the context

**When to Keep It Simple:**
For very small, obvious changes (typo fixes, trivial refactors), you can use a shorter structure:
- Summary
- Context (brief)
- Testing

But never skip the testing instructions.

**When to Be Comprehensive:**
- New features or major functionality
- Complex technical changes
- Performance improvements or optimizations
- Breaking changes or deprecations
- Changes that affect multiple parts of the codebase