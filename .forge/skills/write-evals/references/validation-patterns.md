# Validation Patterns

This reference contains common validation patterns using `jq` for parsing `context.json` files.

## Context.json Structure

The `context.json` file contains conversation messages with tool calls:

```json
{
  "messages": [
    {
      "role": "user",
      "content": "..."
    },
    {
      "role": "assistant",
      "tool_calls": [
        {
          "function": {
            "name": "tool_name",
            "arguments": "{\"param\": \"value\"}"
          }
        }
      ]
    },
    {
      "role": "tool",
      "name": "tool_name",
      "content": "..."
    }
  ]
}
```

## Tool Usage Validations

### Check if specific tool was called

```bash
jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "tool_name")] | length > 0'
```

### Count tool calls

```bash
jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "tool_name")] | length >= N'
```

### Check tool was NOT called

```bash
jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "tool_name")] | length == 0'
```

### Check tool NOT called with specific arguments

```bash
jq -e '
  [.messages[]?.tool_calls[]? |
   select(.function.name == "shell") |
   select(.function.arguments | test("pattern"))
  ] | length == 0
'
```

### Parse tool arguments

```bash
jq -e '
  [.messages[]?.tool_calls[]? |
   select(.function.name == "tool_name") |
   .function.arguments | fromjson
  ] | any
'
```

## Shell Command Validations

### Detect shell commands

```bash
jq -e '
  [.messages[]?.tool_calls[]? |
   select(.function.name == "shell") |
   .function.arguments | fromjson | .command
  ] | any
'
```

### Detect anti-patterns in shell commands

```bash
# Detect find command usage
jq -e '
  [.messages[]?.tool_calls[]? |
   select(.function.name == "shell") |
   .function.arguments | fromjson |
   select(.command | test("\\bfind\\s+"))
  ] | length == 0
'

# Detect find + grep pipeline
jq -e '
  [.messages[]?.tool_calls[]? |
   select(.function.name == "shell") |
   .function.arguments | fromjson |
   select(.command | test("find.*\\|.*grep"))
  ] | length == 0
'

# Detect cd usage (anti-pattern when cwd is available)
jq -e '
  [.messages[]?.tool_calls[]? |
   select(.function.name == "shell") |
   .function.arguments | fromjson |
   select(.command | test("^\\s*cd\\s+"))
  ] | length == 0
'
```

### Verify specific commands were used

```bash
jq -e '
  [.messages[]?.tool_calls[]? |
   select(.function.name == "shell") |
   .function.arguments | fromjson |
   select(.command | test("git\\s+commit"))
  ] | length > 0
'
```

## File Operation Validations

### Check files were read

```bash
jq -e '
  [.messages[]?.tool_calls[]? |
   select(.function.name == "Read") |
   .function.arguments | fromjson | .file_path
  ] | any
'
```

### Verify specific file was modified

```bash
jq -e '
  [.messages[]?.tool_calls[]? |
   select(.function.name == "patch") |
   .function.arguments | fromjson | .file_path
  ] | any | contains("specific_file.rs")
'
```

### Check patch vs shell approach

```bash
# Should use patch tool
jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "patch")] | length > 0'

# Should NOT use git apply via shell
jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "shell" and (.function.arguments | contains("git apply")))] | length == 0'
```

## Output Validations

### Parse tool output

```bash
# Get content from tool result
jq -r '.messages[]? | select(.role == "tool" and .name == "tool_name") | .content'

# Check output contains expected text
jq -e '.messages[]? | select(.role == "tool" and .name == "sem_search") | .content | contains("expected_text")'
```

### Extract and validate results

```bash
# Extract file paths from XML output
jq -r '
  .messages[]? |
  select(.role == "tool") |
  .content' | grep -o 'path="[^"]*"' | sed 's/path="//;s/"//'
```

## LLM Judge Integration

```bash
# Only run if tool was used
if ! cat '{{dir}}/context.json' | jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "tool_name")] | any' > /dev/null 2>&1; then
  echo "Skipping: tool_name was not used"
  exit 0
fi

# Run LLM judge
your_llm_judge_script --context '{{dir}}/context.json' --intent '{{intent}}'
```

## Validation Best Practices

1. **Order matters**: Put cheap validations first (tool existence checks) before expensive ones (LLM judges)

2. **Exit 0 for skips**: If a validation doesn't apply (e.g., no expected_files specified), exit 0

3. **Meaningful error messages**: When failing, output helpful context:
   ```bash
   echo "Missing expected files:"
   printf '  - %s\n' "${missing_files[@]}"
   exit 1
   ```

4. **Use `any` for boolean checks**: When checking if ANY condition is true:
   ```bash
   jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "tool")] | any'
   ```

5. **Use `length` for counting**: For count comparisons:
   ```bash
   jq -e '[.messages[]?.tool_calls[]? | select(.function.name == "tool")] | length >= 3'
   ```
