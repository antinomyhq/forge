Modifies files with targeted line operations on matched patterns. Supports prepend, append, replace, replace_all, swap operations. Ideal for precise changes to configs, code, or docs while preserving context. Use this tool for refactoring tasks (e.g., renaming variables, updating function signatures). For maximum efficiency, invoke multiple `patch` operations simultaneously rather than sequentially. Fails if search pattern isn't found.

Usage Guidelines:
- When editing text from Read tool output, preserve the EXACT text character-by-character (indentation, spaces, punctuation, special characters) as it appears AFTER the line number prefix. Format: 'line_number:'. Never include the prefix.
- CRITICAL: Even tiny differences like 'allows to' vs 'allows the' will fail