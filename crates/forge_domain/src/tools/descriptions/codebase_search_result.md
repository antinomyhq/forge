Report structured codebase search results with selected code chunks and relevance information. Use this tool at the end of codebase search operations to return the final findings.

Each chunk selection requires:
- **file_path**: Absolute path to the file
- **start**: Starting line number (1-based, optional)
- **end**: Ending line number (1-based, optional)
- **reason**: Clear explanation of why this chunk was selected and how it relates to the search query
- **relevance**: Relevance level - "high", "medium", or "low"

Guidelines:
- Call this tool after completing codebase search analysis
- Provide specific, actionable reasons for each chunk selection
- Use relevance levels to help prioritize results:
  - **high**: Directly relevant, critical for the task
  - **medium**: Somewhat relevant, provides context
  - **low**: Tangentially related, background information
- Include line ranges (start/end) when focusing on specific code sections
- Ensure reasons explain the connection to the search query, not just describe the code