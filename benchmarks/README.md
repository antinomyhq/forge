# Forge Code Evaluations

A flexible evaluation framework for running automated tests and benchmarks against Forge Code commands.

## Quick Start

### Setup

Before running evaluations, create a `forgee` symlink to the debug binary:

```bash
# Create symlink in your PATH (e.g., ~/bin or /usr/local/bin)
ln -sf /path/to/code-forge/target/debug/forge ~/forgee

# Or if ~/bin is in your PATH
ln -sf $(pwd)/target/debug/forge ~/bin/forgee
```

**Why is this needed?** Tasks execute in temporary directories, so relative paths like `../../target/debug/forge` won't work. The `forgee` symlink provides a stable absolute path that works from any directory.

### Running Evaluations

```bash
# Run an evaluation
npm run eval ./evals/create_skill/task.yml

# Set custom log level
LOG_LEVEL=debug npm run eval ./evals/create_skill/task.yml
```

## How It Works

The evaluation system executes commands based on task definitions and validates their output. It supports:

- **Parallel execution** with configurable concurrency
- **Timeout handling** for long-running tasks
- **Data-driven testing** using CSV files
- **Output validation** using regex patterns
- **Debug artifacts** stored with timestamps

## Project Structure

```
benchmarks/
├── cli.ts                    # Main CLI entry point
├── command-generator.ts      # Command template rendering
├── task-executor.ts          # Task execution with timeout support
├── model.ts                  # TypeScript types for tasks
├── parse.ts                  # CLI argument parsing
└── evals/                    # Evaluation definitions
    └── create_skill/
        ├── task.yml          # Task definition
        ├── create_skill_tasks.csv  # Test data
        └── debug/            # Debug outputs (timestamped)
```

## Creating an Evaluation

### 1. Create an Evaluation Directory

```bash
mkdir -p evals/my_eval
```

### 2. Create a `task.yml` File

The task file defines how to run your evaluation:

```yaml
# Optional: Commands to run before evaluation starts
before_run:
  - cargo build
  - npm install

# Required: Command(s) to execute for each test case
# Single command
run: forgee -p '{{prompt}}'

# Or multiple commands (executed sequentially)
run:
  - echo "Step 1: {{task}}"
  - forgee -p '{{prompt}}'
  - echo "Step 2: Complete"

# Execution configuration
parallelism: 10  # Number of tasks to run in parallel (default: 1)
timeout: 60      # Timeout in seconds (optional)
early_exit: true # Stop execution when validations pass (optional)

# Optional: Validations to run on output
validations:
  - name: "Check success message"
    type: regex
    regex: \[[0-9:]*\] Skill create-skill

# Required: Data sources for test cases
sources:
  - csv: my_tasks.csv
```

#### Task File Schema

**`before_run`** (optional): Array of shell commands to execute before running tasks
- Runs sequentially before the main command execution
- Executes in a temporary directory created for the evaluation run
- Useful for building binaries or setting up dependencies

**`run`** (required): Command(s) to execute for each test case
- Can be a single string or an array of strings
- Commands support template placeholders (e.g., `{{variable}}`)
- Multiple commands are executed sequentially
- If any command fails, subsequent commands are skipped

**`parallelism`** (optional): Number of tasks to run concurrently (default: 1)

**`timeout`** (optional): Maximum execution time in seconds per task

**`early_exit`** (optional): Stop command execution when all validations pass

**`validations`** (optional): Array of validation rules
- `name`: Human-readable description
- `type`: Validation type. Supported values:
  - `regex`: Match output against a regular expression pattern
  - `shell`: Execute a shell command with output as stdin
- For `regex` type:
  - `regex`: Regular expression pattern to match in output
- For `shell` type:
  - `command`: Shell command to execute (receives task output via stdin)
  - `exit_code`: Expected exit code (default: 0)

**`sources`** (required): Array of data sources
- Currently supports CSV files: `- csv: filename.csv`
- Future: Command output: `- cmd: command`

### 3. Create Test Data (CSV)

Create a CSV file with columns matching your template variables:

```csv
prompt,expected_output
"Create a backup script","backup.sh"
"Generate API client","api_client.py"
```

The column names (e.g., `prompt`, `expected_output`) become template variables you can use in your command with `{{column_name}}`.

### 4. Run the Evaluation

```bash
npm run eval ./evals/my_eval/task.yml
```

## Template Variables

Commands support Handlebars template syntax. Variables are populated from CSV columns:

```yaml
run:
  command: ./tool --input '{{input_file}}' --format {{format}} --verbose
```

With CSV:
```csv
input_file,format
data1.txt,json
data2.txt,yaml
```

## Output and Debugging

### Debug Artifacts

Each run creates a timestamped debug directory:

```
evals/my_eval/debug/2025-11-23T10-30-45-123Z/
├── task-1.log
├── task-2.log
└── task-3.log
```

Each log file contains the full output (stdout + stderr) from the command execution.

### Task Status

Tasks can have four statuses:

- **`passed`**: Task completed successfully and passed all validations
- **`validation_failed`**: Task completed but failed one or more validations
- **`timeout`**: Task exceeded the timeout limit
- **`failed`**: Task execution failed (non-zero exit code)

### Logging

**Human-readable output (default):**
```bash
npm run eval ./evals/my_eval/task.yml
```

**Machine-readable JSON output:**
```bash
LOG_JSON=1 npm run eval ./evals/my_eval/task.yml | jq .
```

**Debug logging:**
```bash
LOG_LEVEL=debug npm run eval ./evals/my_eval/task.yml
```

## Examples

### Example 1: Simple Sequential Execution

```yaml
run: echo "Processing {{name}}"
sources:
  - csv: names.csv
```

```csv
name
Alice
Bob
Charlie
```

### Example 2: Parallel Execution with Timeout

```yaml
run: ./slow_task --id {{task_id}}
parallelism: 5
timeout: 30
sources:
  - csv: tasks.csv
```

### Example 3: Multiple Commands

```yaml
run:
  - echo "Starting task {{id}}"
  - ./process --input {{file}}
  - echo "Task {{id}} complete"
parallelism: 3
timeout: 120
sources:
  - csv: tasks.csv
```

### Example 4: Shell Command Validation

```yaml
run: echo "{{message}}"
parallelism: 3
validations:
  # Using grep to check if output contains specific text
  - name: "Contains 'test' word"
    type: shell
    command: grep -q "test"
    exit_code: 0
  
  # Count words and ensure it's greater than 2
  - name: "More than 2 words"
    type: shell
    command: test $(wc -w | awk '{print $1}') -gt 2
    exit_code: 0
  
  # Traditional regex validation (for comparison)
  - name: "Contains test or validation"
    type: regex
    regex: "test|validation"
sources:
  - csv: messages.csv
```

### Example 5: Regex Validation

```yaml
run: cargo test {{test_name}}
validations:
  - name: "All tests passed"
    type: regex
    regex: test result:\s+ok
sources:
  - csv: tests.csv
```

## Tips

1. **Use quotes in commands**: When passing CSV values with spaces, wrap them in quotes:
   ```yaml
   command: forge -p '{{prompt}}'
   ```

2. **Build before running**: Use `before_run` to ensure binaries are up-to-date:
   ```yaml
   before_run:
     - cargo build
   ```

3. **Start with low parallelism**: Test with `parallelism: 1` first, then increase:
   ```yaml
   parallelism: 1  # Start here
   ```

4. **Set appropriate timeouts**: Add timeouts to prevent hanging:
   ```yaml
   timeout: 60  # seconds
   ```

5. **Check debug logs**: When tasks fail, check the debug directory for full output:
   ```bash
   cat evals/my_eval/debug/*/task-1.log
   ```

## Exit Codes

The CLI exits with:
- **0**: All tasks passed
- **1**: One or more tasks failed (excluding timeouts and validation failures)
