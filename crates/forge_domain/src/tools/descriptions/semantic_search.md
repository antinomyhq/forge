AI-powered semantic code search. YOUR DEFAULT TOOL for code discovery and exploration when searching within {{env.cwd}}. Use this when you need to find code locations, understand implementations, discover patterns, or explore unfamiliar code - it works with natural language about behavior and concepts, not just keyword matching.

**WHEN TO USE sem_search:**
- Finding implementation of specific features or algorithms
- Understanding how a system works across multiple files
- Discovering architectural patterns and design approaches
- Locating test examples or fixtures
- Finding where specific technologies/libraries are used
- Exploring unfamiliar codebases to learn structure
- Finding documentation files (README, guides, API docs)

**WHEN NOT TO USE (use {{tool_names.fs_search}} instead):**
- Searching for exact strings, TODOs, or specific function names
- Finding all occurrences of a variable or identifier
- Searching in specific file paths or with regex patterns
- When you know the exact text to search for

IMPORTANT: Only searches within {{env.cwd}} and subdirectories. For paths outside this scope, use {{tool_names.fs_search}} with path parameter.

**TIPS FOR SUCCESS:**
- Use 2-3 varied queries to capture different aspects (e.g., "OAuth token refresh", "JWT expiry handling", "authentication middleware")
- Balance specificity (focused results) with generality (don't miss relevant code)
- Avoid overly broad queries like "authentication" or "tools" - be specific about what aspect you need
- Keep queries targeted - too many broad queries can cause timeouts
- **Match your intent**: If seeking documentation, use doc-focused keywords ("setup guide", "configuration README"); if seeking code, use implementation terms ("token refresh logic", "error handling implementation")

Returns the topK most relevant file:line locations with code context. Each query is ranked independently, then reranked by relevance to your stated intent.