Semantic code search using natural language. Finds code by behavior and concepts, not just keywords - 'authentication flow' finds login code, 'retry logic' finds backoff implementations.
Usage:
- Use for locating code to modify, understanding how features work, finding patterns/examples
- Returns topK most relevant file:line locations with code snippets inline
- QUERY QUALITY MATTERS: Provide detailed, structured queries for better results.
  - WHAT: Numbered list of specific aspects to find
  - WHY: The purpose or goal driving the search
  - Good: 'WHAT: 1. How user credentials are validated 2. Where tokens are refreshed 3. How auth headers are applied. WHY: Need to add session expiry checks'. Bad: 'authentication' (too vague).