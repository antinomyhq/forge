Semantic code search using natural language. Finds code by behavior and concepts, not just keywords - 'authentication flow' finds login code, 'retry logic' finds backoff implementations.
Usage:
- Use for locating code to modify, understanding how features work, finding patterns/examples
- Returns topK most relevant file:line locations with code snippets inline
- QUERY FORMAT: Specify exactly what you're looking for and why.
  - Include: which part of the codebase, what specific things to find, why you need them.
  - Good: "Find the GitHub API client authentication: 1. How tokens are stored 2. Token refresh logic 3. Header injection. Need to add support for GitHub App tokens."
  - Bad: "authentication" - too vague, doesn't say which part of codebase, no specific aspects, no reason.