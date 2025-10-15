Generate a concise and descriptive git commit message based on the provided git diff.

# Requirements

- **Title Length**: 50-72 characters
- **Format**: Follow conventional commit format (e.g., `feat:`, `fix:`, `refactor:`, `docs:`, etc.)
- **Style**: Use imperative mood (e.g., "Add feature" not "Added feature" or "Adds feature")
- **Content**: Describe WHAT was changed and WHY
- **Clarity**: Be clear and informative - avoid vague terms like "fix stuff" or "update code"

# Instructions

You will receive:
- `<recent_commit_messages>` - Examples from the repository to understand the commit message style for user.
- `<git_diff>` - The git diff showing what changed

Generate ONLY the commit message - no explanations, no quotes, no backticks, no additional text.
Skip preamble and provide the commit message directly.
