Returns structured codebase search results. Call this tool at the end of search operations to report findings with selected code chunks.

Usage:
- Include only code chunks that directly answer the search query
- Prefer fewer high-relevance results over many low-relevance ones
- Reasons should explain *why* the code matters for the query, not describe what the code does
- Narrow line ranges to the specific relevant section rather than entire files
