---
name: create-github-issue
description: Create GitHub issues using GitHub CLI with support for templates, labels, assignees, milestones, and draft issues. Use when the user asks to create a GitHub issue, file a bug report, submit a feature request, or open an issue in a GitHub repository.
---

# Create GitHub Issue

Create comprehensive GitHub issues using `gh issue create` by dynamically discovering and adhering to the repository's official issue templates.

## Workflow

### 1. Discover and Select Template

**CRITICAL**: You must use the repository's official templates. Do not assume the structure.

1.  **List available templates**:
    ```bash
    ls .github/ISSUE_TEMPLATE/
    ```
2.  **Select the most appropriate template** based on the issue type (e.g., `bug_report.yml`, `feature_request.yml`).
3.  **Read the selected template** to understand its required fields, structure, and any title prefixes or default labels.
    ```bash
    cat .github/ISSUE_TEMPLATE/<selected_template>.yml
    ```

### 2. Gather Context

Gather relevant information to fulfill the template requirements:

```bash
# Check current git status for context
git status

# View recent commits if related to codebase changes
git log --oneline -10

# Check if related issues exist
gh issue list --search "keyword" --limit 10
```

### 3. Generate Markdown Body

**MANDATORY**: Structure the issue body exactly as defined in the selected YAML template without exception.

1.  **Map YAML fields to Markdown**: Convert each field in the template (typically found under `body:`) into a Markdown section.
2.  **Use Headers**: Use the `label` or `id` from the YAML field as an H2 header (e.g., `## Bug Description`).
3.  **Respect Validation**: Ensure all fields marked as `required: true` in the YAML are populated with meaningful content.
4.  **Format Correctly**: Use code blocks (e.g., ` ```shell `, ` ```yaml `) for logs and configurations as suggested by the template's `render` attribute.

### 4. Choose Labels

Select labels by inspecting `.github/labels.json`.

- **Primary Label**: Always include a `type:` label that matches the issue type.
- **Additional Labels**: Add relevant `state:`, `work:`, or `priority:` labels if they exist in the configuration.
- **Constraint**: **Only use labels defined in `.github/labels.json`.**

### 5. Create Title

Follow the template's `title` field if it provides a prefix (e.g., `"[Bug]: "`).

- **Be concise**: Keep under 70 characters.
- **Use imperative mood**: "Fix authentication timeout" instead of "Authentication is timing out".
- **Start with action verb**: Fix, Add, Improve, Update, Refactor.

### 6. Execute Issue Creation

**Step 1: Write body to temp file**
Use the `write` tool to create `.forge/FORGE_ISSUE_BODY.md` with the structured Markdown content.

**Step 2: Create issue**
```bash
gh issue create \
  --title "[Prefix]: Descriptive Title" \
  --body-file .forge/FORGE_ISSUE_BODY.md \
  --label "type: <type>, work: <complexity>"
```

**Optional flags**:
- `--assignee "username"`
- `--milestone "name"`
- `--draft` (For proposals or research)

### 7. Finalize

Provide the user with the generated issue URL and a brief summary of the created issue.

## Guidelines

- **No Exceptions**: You must follow the discovered template's structure exactly. If a template asks for "Steps to Reproduce", you must provide them.
- **Dynamic Discovery**: Always read the files in `.github/ISSUE_TEMPLATE/` and `.github/labels.json` first. Never rely on hardcoded knowledge of templates or labels as they change frequently.
- **Cleanliness**: Ensure no placeholder text (like "Describe the bug...") remains in the final body.
- **Contextual Awareness**: If the user provides logs or code snippets, ensure they are placed in the correct sections of the template.

## Notes

- **Single Source of Truth**: The files in `.github/` are the authoritative reference for issue structure and categorization.
- **Tooling**: Use `gh` CLI directly. It is pre-authenticated and ready for use.
- **Automation**: Do not ask for confirmation before creating the issue if the user's intent is clear.