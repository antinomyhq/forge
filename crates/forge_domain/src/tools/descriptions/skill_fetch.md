Fetches detailed information about a specific skill. Use this tool to load a skill's full content when its summary (listed in the `<system_reminder>` catalog) matches the user's request.

Skills provide domain-specific knowledge, reusable workflows, and best practices. The list of available skills (name + short description) is delivered to you as a `<system_reminder>` message at the start of each turn, and refreshed whenever new skills become available mid-session.

**Usage rules:**

- Only invoke skills whose name appears in the most recent `<system_reminder>` catalog.
- Do not invoke a skill that is already active (i.e. whose content has already been loaded in the current turn).
- When a skill matches the user's intent, prefer invoking it over reasoning from scratch — skills encode battle-tested workflows.
- The tool returns the full SKILL.md content including any frontmatter-declared resources. Read and follow the instructions it contains.
