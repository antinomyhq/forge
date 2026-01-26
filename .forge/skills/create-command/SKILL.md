---
name: create-command
description: Create new commands for the code-forge application. Commands are stored as .md files in the commands directory with YAML frontmatter (name, description) and markdown body containing command steps. Use when users need to add new commands, modify existing commands, or understand the command file structure. Supports special command tags like <lint> and <test> for automated workflows.
---

# Create Commands

Create and manage commands for the code-forge application. Commands are modular workflows that can be invoked to perform specific tasks.

## Command File Structure

Every command file must have:

1. **YAML Frontmatter** (required):
   - `name`: Command identifier (use hyphens for multi-word names)
   - `description`: What the command does

2. **Command Body** (required):
   - List of steps to execute
   - Special tags for automated workflows
   - Clear instructions for each step

### Example Command File

```markdown
---
name: check
description: Checks if the code is ready to be committed
---

- Run the `lint` and `test` commands and verify if everything is fine.
  <lint>cargo +nightly fmt --all; cargo +nightly clippy --fix --allow-staged --allow-dirty --workspace</lint>
  <test>cargo insta test --accept --unreferenced=delete</test>
- Fix every issue found in the process
```

### Complete Sample Command

This sample demonstrates all three tag types:

```markdown
---
name: sample-command
description: Sample command demonstrating the command file structure
---

This is a sample command that demonstrates the structure of command files.

- First step: Perform an initial action
  <lint>echo "Running linting..."</lint>
- Second step: Execute tests
  <test>echo "Running tests..."</test>
- Third step: Complete the workflow
  <shell>echo "Workflow complete!"</shell>
- Final step: Verify everything worked correctly
```

## Creating a New Command

### Step 1: Determine Command Purpose

Identify what the command should accomplish:

- What task will it perform?
- What steps are involved?
- Are there automated checks or tests needed?
- What should the user do with the results?

### Step 2: Choose Command Name

Use verb-based names with hyphens for multi-word commands:

- Good: `check`, `fixme`, `pr-description`, `run-tests`
- Bad: `checker`, `fixing`, `PRdescription`

### Step 3: Write the Command File

Create the file in the commands directory with the format: `{command-name}.md`

#### Frontmatter

```yaml
---
name: your-command-name
description: Clear, concise description of what this command does
---
```

#### Command Body

Use markdown lists for steps. Each step should:

- Start with a clear action verb
- Be specific and actionable
- Include context about what to do

## Special Command Tags

Use these tags for automated workflows:

### `<lint>` Tag

For linting/formatting commands:

```markdown
<lint>cargo +nightly fmt --all; cargo +nightly clippy --fix --allow-staged --allow-dirty --workspace</lint>
```

### `<test>` Tag

For testing commands:

```markdown
<test>cargo insta test --accept --unreferenced=delete</test>
```

### `<shell>` Tag

For general shell commands (not linting or testing):

```markdown
<shell>rm -rf target/debug</shell>
```

### Using Tags

Tags should be placed on their own line after the step description:

```markdown
- Run linting and testing
  <lint>your-lint-command</lint>
  <test>your-test-command</test>
```

## Command Types

### Simple Commands

Single-step or instruction-only commands:

```markdown
---
name: fixme
description: Looks for all the fixme comments in the code and attempts to fix them
---

Find all the FIXME comments in source-code files and attempt to fix them.
```

### Multi-Step Commands

Commands with multiple sequential steps:

```markdown
---
name: pr-description
description: Updates the description of the PR
---

- I have created a Pull Request with all the accepted changes
- Understand the current PR deeply using the GH CLI and update the PR title and description
- Make sure the title follows conventional commits standard
- Top-level summary should contain 2-3 lines about the core functionality improvements
```

### Automated Workflow Commands

Commands that include automated checks:

```markdown
---
name: check
description: Checks if the code is ready to be committed
---

- Run the `lint` and `test` commands and verify if everything is fine.
  <lint>cargo +nightly fmt --all; cargo +nightly clippy --fix --allow-staged --allow-dirty --workspace</lint>
  <test>cargo insta test --accept --unreferenced=delete</test>
- Fix every issue found in the process
```

## Command Templates

### Simple Command Template

```markdown
---
name: simple-command
description: Does one specific thing
---

Single clear instruction or description.
```

### Automated Workflow Template

```markdown
---
name: automated-workflow
description: Runs automated checks and performs follow-up actions
---

- Run automated checks
  <lint>your-lint-command</lint>
  <test>your-test-command</test>
- Review and fix any issues found
- Complete the workflow
```

### Multi-Step Workflow Template

```markdown
---
name: multi-step-workflow
description: Performs multiple sequential steps
---

- First step with clear action
- Second step with context
- Third step with specific requirements
- Final step with verification
```

### Git Workflow Template

```markdown
---
name: git-workflow
description: Performs git operations
---

- Stage changes
  <shell>git add .</shell>
- Run pre-commit checks
  <lint>cargo fmt --all</lint>
  <test>cargo test</test>
- Commit with message
  <shell>git commit -m "your commit message"</shell>
- Push to remote
  <shell>git push</shell>
```

## Best Practices

### Naming

- Use lowercase letters
- Use hyphens to separate words
- Use verb-based names (imperative form)
- Keep names short but descriptive

### Descriptions

- Be clear and concise
- Describe what the command does, not how
- Include the main purpose and key outcomes
- Avoid implementation details

### Command Steps

- Use numbered lists for sequential steps
- Start each step with an action verb
- Be specific about what to do
- Include context for complex steps
- Use present tense

### Special Tags

- Place tags on their own line after the step
- Only use `<lint>`, `<test>`, and `<shell>` tags
- Include complete commands that can be executed
- Use appropriate flags for your workflow

## Common Patterns

### Git Workflow Commands

```markdown
---
name: commit-check
description: Verifies code is ready to commit
---

- Run linting and tests
  <lint>cargo fmt --all; cargo clippy --fix --allow-staged</lint>
  <test>cargo test</test>
- Review and fix any issues
- Stage all changes
```

### Documentation Commands

```markdown
---
name: update-docs
description: Updates documentation for recent changes
---

- Review recent code changes
- Identify functions or modules that need documentation
- Update inline documentation comments
- Regenerate any auto-generated docs
- Verify documentation builds successfully
```

### Cleanup Commands

```markdown
---
name: cleanup
description: Cleans up temporary files and artifacts
---

- Remove build artifacts
  <shell>rm -rf target/debug</shell>
- Remove temporary files
  <shell>find . -name "*.tmp" -delete</shell>
- Clean up dependency caches if needed
- Verify the project still builds
```

### Build and Deploy Commands

```markdown
---
name: build-deploy
description: Builds the project and deploys to staging
---

- Build the project in release mode
  <shell>cargo build --release</shell>
- Run integration tests
  <test>cargo test --test integration</test>
- Build Docker image
  <shell>docker build -t myapp:latest .</shell>
- Tag image for staging
  <shell>docker tag myapp:latest myapp:staging</shell>
- Push to registry
  <shell>docker push myapp:staging</shell>
- Deploy to staging environment
  <shell>kubectl set image deployment/myapp myapp=myapp:staging</shell>
```

## Validation Checklist

Use this checklist to verify your command is complete and correct:

### File Structure

- [ ] File is in the commands directory
- [ ] Filename matches command name (e.g., `check.md` for `name: check`)
- [ ] File has `.md` extension
- [ ] YAML frontmatter uses `---` delimiters

### Frontmatter

- [ ] `name` field is present
- [ ] `name` uses lowercase letters
- [ ] `name` uses hyphens for multi-word names
- [ ] `name` is verb-based (imperative form)
- [ ] `description` field is present
- [ ] `description` is clear and concise
- [ ] `description` describes what, not how

### Command Body

- [ ] At least one step is defined
- [ ] Steps use bullet points (`-`)
- [ ] Each step starts with action verb
- [ ] Steps are specific and actionable
- [ ] Complex steps include context
- [ ] Steps are in logical order

### Special Tags

- [ ] Tags are on their own line after step description
- [ ] Only valid tags are used (`<lint>`, `<test>`, `<shell>`)
- [ ] Tag commands are complete and executable
- [ ] Tag commands use appropriate flags
- [ ] Tag commands are properly formatted

### Content Quality

- [ ] Command name is descriptive
- [ ] Steps are clear and unambiguous
- [ ] No redundant or duplicate steps
- [ ] Steps follow logical sequence
- [ ] Special requirements are documented
- [ ] Error handling is considered

### Testing

- [ ] Command can be executed successfully
- [ ] All steps complete as expected
- [ ] Special tags work correctly
- [ ] Output is as expected
- [ ] Edge cases are handled
- [ ] **Command is recognized by forge**: Run `forge list command --custom` (or `forge list cmd`) and verify your command appears in the list

## Common Mistakes to Avoid

### Frontmatter Mistakes

Bad: **Wrong delimiter**:

```markdown
---
name: my-command
description: My command
```

(Missing closing `---`)

Good: **Correct**:

```markdown
---
name: my-command
description: My command
---
```

Bad: **Missing required field**:

```markdown
---
name: my-command
---
```

(Missing `description`)

Good: **Correct**:

```markdown
---
name: my-command
description: Does something useful
---
```

### Naming Mistakes

Bad: **CamelCase name**:

```markdown
---
name: myCommand
description: Does something
---
```

Good: **Correct**:

```markdown
---
name: my-command
description: Does something
---
```

Bad: **Noun instead of verb**:

```markdown
---
name: checker
description: Checks something
---
```

Good: **Correct**:

```markdown
---
name: check
description: Checks something
---
```

### Step Mistakes

Bad: **No action verb**:

```markdown
---
name: test
description: Runs tests
---

- The tests
- The code
```

Good: **Correct**:

```markdown
---
name: test
description: Runs tests
---

- Run all tests
- Verify code quality
```

Bad: **Vague steps**:

```markdown
---
name: deploy
description: Deploys application
---

- Do the deployment
- Make sure it works
```

Good: **Correct**:

```markdown
---
name: deploy
description: Deploys application to production
---

- Build the Docker image
  <shell>docker build -t myapp:latest .</shell>
- Push to registry
  <shell>docker push myapp:latest</shell>
- Deploy to production
  <shell>kubectl set image deployment/myapp myapp=myapp:latest</shell>
- Verify deployment is healthy
```

### Tag Mistakes

Bad: **Tag on same line**:

```markdown
- Run tests <test>cargo test</test>
```

Good: **Correct**:

```markdown
- Run tests
  <test>cargo test</test>
```

Bad: **Invalid tag**:

```markdown
- Run checks
  <check>cargo clippy</check>
```

Good: **Correct**:

```markdown
- Run checks
  <lint>cargo clippy</lint>
```

Bad: **Incomplete command**:

```markdown
- Format code
  <lint>cargo fmt</lint>
```

(Missing `--all` flag)

Good: **Correct**:

```markdown
- Format code
  <lint>cargo fmt --all</lint>
```

## Quick Reference

### Valid Tags

- `<lint>` - For linting/formatting commands
- `<test>` - For testing commands
- `<shell>` - For general shell commands

### Naming Rules

- Lowercase only
- Hyphens for multi-word names
- Verb-based (imperative form)
- Keep it short but descriptive

### Step Guidelines

- Start with action verb
- Be specific
- Include context for complex steps
- Use present tense
- Keep steps focused

### When to Use Tags

- Use `<lint>` when running formatters or linters
- Use `<test>` when running test suites
- Use `<shell>` for other shell commands
- Place tags on their own line after step description
- Don't use tags if the step is just an instruction

## Testing Your Command

After creating a command, test it by:

1. **Syntax Check**: Verify YAML is valid

   ```bash
   # If you have yamllint installed
   yamllint path/to/your-command.md
   ```

2. **Manual Review**: Read through the command
   - Does each step make sense?
   - Is the order logical?
   - Are all commands complete?

3. **Execution Test**: Run the command
   - Does each step execute successfully?
   - Is the output as expected?
   - Are there any errors?

4. **Forge Recognition Test**: Verify the command is recognized by forge

   ```bash
   # Option 1: List all commands (custom commands marked as type: custom)
   forge list command

   # Option 2: List only custom commands
   forge list cmd

   # Option 3: List only custom commands (newer versions)
   forge list command --custom
   ```

   - Does your command appear in the list?
   - Is the name correct?
   - Is the description correct?

5. **Edge Cases**: Consider unusual scenarios
   - What happens if a step fails?
   - What if the environment is different?
   - What if files are missing?

## Verification

After creating a command:

1. Verify the file is in the commands directory
2. Check YAML frontmatter is valid (use `---` delimiters)
3. Ensure the command name matches the filename (without .md)
4. **Verify the command is recognized by forge**:

   ```bash
   # Option 1: List all commands (custom commands marked as type: custom)
   forge list command

   # Option 2: List only custom commands
   forge list cmd

   # Option 3: List only custom commands (newer versions)
   forge list command --custom
   ```

   Your new command should appear in the list with its name and description

5. Test the command to ensure it works as expected
6. Verify special tags are properly formatted

If your command doesn't appear in the list, check:

- File is in the commands directory
- Filename matches the `name` field in frontmatter
- YAML frontmatter is properly formatted with `---` delimiters
- Both `name` and `description` fields are present

## Getting Help

If you're unsure about something:

- Review the examples in this skill
- Follow the validation checklist
- Test your command before finalizing