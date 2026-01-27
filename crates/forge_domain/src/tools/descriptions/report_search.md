Report search results to the user. Returns code chunks ordered by relevance (most relevant first). Line ranges should contain only the relevant code, not entire files.

Usage:
- Include only code chunks that directly answer the search query
- Prefer fewer high-relevance results over many low-relevance ones
- Reasons should explain *why* the code matters for the query, not describe what the code does
- Narrow line ranges to the specific relevant section rather than entire files
