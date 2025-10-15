Generate a concise, single-line git commit message based on the provided git diff.

# Requirements

- **Length**: Maximum 72 characters (aim for 50-60 for best practice)
- **Style**: Use imperative mood (e.g., "Add feature" not "Added feature" or "Adds feature")
- **Content**: Describe WHAT changed and WHY (if not obvious from the what)
- **Format**: Single line, no period at the end
- **Clarity**: Be specific and actionable - avoid vague terms like "fix stuff" or "update code"

# Instructions

You will receive:
- `<recent_commit_messages>` - Examples from the repository to understand the commit message style
- `<git_diff>` - The git diff showing what changed

Generate ONLY the commit message - no explanations, no quotes, no additional text.
The message should be clear enough that someone reading it in the git log understands the change without viewing the diff.
