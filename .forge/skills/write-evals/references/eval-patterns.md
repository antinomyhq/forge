# Eval Patterns

This reference contains patterns and examples for creating different types of evals.

## Table of Contents

1. [Basic Structure](#basic-structure)
2. [Simple Output Eval](#simple-output-eval)
3. [Tool Usage Eval](#tool-usage-eval)
4. [Multi-file Patch Eval](#multi-file-patch-eval)
5. [Anti-pattern Detection Eval](#anti-pattern-detection-eval)
6. [Complex Validation with LLM Judge](#complex-validation-with-llm-judge)
7. [Task Parameterization](#task-parameterization)

---

## Basic Structure

Every eval needs:

```yaml
run:
  # Commands to set up and run the task
  - git clone https://github.com/repo .

parallelism: 10          # Concurrent task executions
timeout: 120             # Seconds per task
early_exit: true         # Stop after first failure

validations:
  - name: "Description"
    type: shell           # or "regex"
    command: "jq '...'"

sources:
  - csv: tasks.csv       # or - value: [...] for inline
```

---

## Simple Output Eval

**Use when**: Checking that a task produces expected output text.

```yaml
run:
  - echo '{{message}}'

parallelism: 5
timeout: 10

validations:
  - name: "Output contains expected text"
    type: regex
    regex: "Hello"
  - name: "Output is not empty"
    type: shell
    command: 'grep -q "."'
    exit_code: 0

sources:
  - value:
      - message: "Hello, World!"
      - message: "Hello from test"
```

**Key points**:
- `regex` type: Fast, no command execution
- `shell` type: More flexible, can chain commands
- `exit_code: 0` means grep found a match

---

## Tool Usage Eval

**Use when**: Verifying that the agent uses specific tools.

```yaml
run:
  - git clone --depth=1 --branch main https://github.com/repo .
  - FORGE_DEBUG_REQUESTS='{{dir}}/context.json' forgee -p '{{task}}'

parallelism: 10
timeout: 180
early_exit: true

validations:
  # Positive: Should use specific tool
  - name: "Uses semantic search"
    type: shell
    command: |
      jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "sem_search")] | any'
      '{{dir}}/context.json'

  # Positive: Should use multiple specific tools
  - name: "Uses search and patch"
    type: shell
    command: |
      jq -e '
        [.messages[]?.tool_calls[]?.function.name] | 
        any(. == "fs_search") and any(. == "patch")
      ' '{{dir}}/context.json'

sources:
  - csv: tasks.csv
```

---

## Multi-file Patch Eval

**Use when**: Verifying patch tool usage across multiple files.

```yaml
run:
  - git clone --depth=1 --branch main https://github.com/repo .
  - FORGE_DEBUG_REQUESTS='{{dir}}/context.json' forgee -p '{{task}}'

parallelism: 50
timeout: 120
early_exit: true

validations:
  - name: "Uses patch tool"
    type: shell
    command: |
      jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "patch")] | length > 0'
      '{{dir}}/context.json'

  - name: "Does NOT use git apply"
    type: shell
    command: |
      jq -e '
        [.messages[]?.tool_calls[]? |
         select(.function.name == "shell" and 
                (.function.arguments | contains("git apply") or 
                              contains("git patch")))] | 
        length == 0
      ' '{{dir}}/context.json'

  - name: "Patches multiple files"
    type: shell
    command: |
      jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "patch")] | length >= 3'
      '{{dir}}/context.json'

sources:
  - value:
      - model: "openai/gpt-5.2"
  - csv: multi_file_patch_tasks.csv
```

---

## Anti-pattern Detection Eval

**Use when**: Ensuring agent avoids specific anti-patterns.

```yaml
run:
  - git clone --depth=1 --branch main https://github.com/repo .
  - FORGE_DEBUG_REQUESTS='{{dir}}/context.json' forgee -p '{{task}}'

parallelism: 10
timeout: 180
early_exit: true

validations:
  # Anti-pattern: Should NOT use find command
  - name: "No shell find command"
    type: shell
    command: |
      jq -e '
        [.messages[]?.tool_calls[]? |
         select(.function.name == "shell") |
         .function.arguments | fromjson |
         select(.command | test("\\bfind\\s+"))
        ] | length == 0
      ' '{{dir}}/context.json'

  # Anti-pattern: Should NOT use find + grep
  - name: "No find + grep pipeline"
    type: shell
    command: |
      jq -e '
        [.messages[]?.tool_calls[]? |
         select(.function.name == "shell") |
         .function.arguments | fromjson |
         select(.command | test("find.*\\|.*grep"))
        ] | length == 0
      ' '{{dir}}/context.json'

  # Positive: Should use proper tools
  - name: "Uses fs_search"
    type: shell
    command: |
      jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "fs_search")] | length > 0'
      '{{dir}}/context.json'

sources:
  - csv: find_vs_search_tasks.csv
```

---

## Complex Validation with LLM Judge

**Use when**: Needing semantic evaluation beyond pattern matching.

```yaml
before_run:
  # Install dependencies for LLM judge
  - npm install --no-save ai @ai-sdk/google-vertex zod

run:
  - git clone --depth=1 --branch main https://github.com/repo .
  - forgee workspace sync
  - FORGE_DEBUG_REQUESTS='{{dir}}/context.json' forgee -p '{{task}}'

parallelism: 30
timeout: 120
early_exit: true

validations:
  - name: "Uses semantic search"
    type: shell
    command: |
      cat '{{dir}}/context.json' | jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "sem_search")] | any'

  - name: "Expected files returned"
    type: shell
    command: |
      # Skip if no expected files specified
      if [ -z "{{expected_files}}" ]; then
        exit 0
      fi
      # ... validation logic ...

  - name: "LLM Judge: Result Quality"
    type: shell
    command: |
      # Only run if tool was used
      if ! cat '{{dir}}/context.json' | jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "sem_search")] | any' > /dev/null 2>&1; then
        echo "Skipping: sem_search not used"
        exit 0
      fi
      tsx llm_judge.ts --context '{{dir}}/context.json' --intent '{{intent}}'

sources:
  - value:
      - model: "anthropic/claude-sonnet-4.5"
  - value:
      - task: "How does workspace sync work?"
        intent: "implementation"
        expected_files: "crates/forge_app/src/workspace_status.rs"
```

---

## Task Parameterization

**Use when**: Running same task with different inputs/models.

### CSV Source

```yaml
sources:
  - csv: tasks.csv
```

```csv
task,model,expected_tool
"Find Rust files","anthropic/claude-sonnet-4.5","fs_search"
"Search for tests","openai/gpt-5.2","sem_search"
```

### Inline Value Source

```yaml
sources:
  - value:
      - model: "anthropic/claude-sonnet-4.5"
      - model: "openai/gpt-5.2"
  - value:
      - task: "Find all .rs files"
      - task: "Search for error handling patterns"
```

### Cross Product (all combinations)

```yaml
sources:
  - value:
      - model: "anthropic/claude-sonnet-4.5"
      - model: "openai/gpt-5.2"
  - value:
      - task: "Find Rust files"
      - task: "Find Python files"
```

This runs 2 models × 2 tasks = 4 combinations.

---

## Setup Patterns

### Git Clone

```yaml
run:
  - git clone --depth=1 --branch main https://github.com/repo .
  - git clone --depth=1 --branch main https://github.com/repo tmp/task
```

### Environment Setup

```yaml
run:
  - git clone ...
  - forgee workspace sync
  - FORGE_DEBUG_REQUESTS='{{dir}}/context.json' forgee -p '{{task}}'
```

### Before Run (dependencies)

```yaml
before_run:
  - npm install --no-save ai @ai-sdk/google-vertex zod

run:
  - # ... tasks ...
```

---

## Performance Tips

1. **`early_exit: true`**: Stop after first validation failure
2. **`parallelism`**: Increase for faster eval runs (balance with API rate limits)
3. **`timeout`**: Set appropriately—too short causes false failures, too long wastes time
4. **Order validations**: Put cheap checks first (tool existence) before expensive ones (LLM judge)

---

## Common Pitfalls

### Wrong jq paths

```bash
# ❌ Wrong - missing ?
jq -e '.messages[].tool_calls[] | select(.function.name == "x")'

# ✅ Correct
jq -e '.messages[]?.tool_calls[]? | select(.function.name == "x")'
```

### Forgetting to quote paths

```bash
# ❌ Wrong
jq -e '...' {{dir}}/context.json

# ✅ Correct - single quotes around jq expression
jq -e '...' '{{dir}}/context.json'
```

### Regex escaping

```bash
# ❌ Wrong - unescaped
jq -e 'select(.command | test("find "))'

# ✅ Correct - escaped for jq
jq -e 'select(.command | test("find\\s+"))'
```
