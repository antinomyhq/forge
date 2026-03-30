# Job Folder Structure

A job folder (e.g., `jobs/sanitize-git-repo__boFyLqA/`) contains all artifacts from a single benchmark trial.

## Top-level files

### `config.json`
Task configuration. Key fields:
- `task.path` -- benchmark task name (e.g., "sanitize-git-repo")
- `task.git_url` -- source benchmark repository
- `task.source` -- benchmark suite (e.g., "terminal-bench")
- `trial_name` -- unique trial identifier
- `agent.import_path` -- agent implementation class

### `result.json`
Trial outcome. Key fields:
- `task_name`, `trial_name` -- identifiers
- `verifier_result.rewards.reward` -- 0.0 (fail) or 1.0 (pass)
- `exception_info` -- non-null if agent crashed
- `agent_execution.started_at` / `finished_at` -- agent wall time
- `agent_result.cost_usd` -- API cost (if tracked)

### `trial.log`
Raw execution log from the benchmark harness.

## `agent/` directory

### `agent/install.sh`
Setup script that installs the agent into the Docker environment.

### `agent/setup/`
- `stdout.txt` -- setup script output
- `return-code.txt` -- setup exit code

### `agent/command-N/` (N = 0, 1, 2, ...)
Each numbered directory is a shell command executed in sequence.

- `command.txt` -- the exact shell command that was run
- `return-code.txt` -- exit code
- `stdout.txt` -- command stdout (may be very large)
- `dump.json` -- conversation dump (JSON, present when `forge conversation dump` was run)
- `dump.html` -- HTML conversation dump (present when `--html` flag was used)

**Typical command sequence:**
- `command-0`: copies AGENTS.md / task prompt into workspace
- `command-1`: runs `forge` with the user prompt (this is the main agent execution)
- `command-2`: runs `forge conversation dump <id>` to export JSON dump
- `command-3`: runs `forge conversation dump <id> --html` to export HTML dump
- `command-4`: `cat *-dump.json` -- outputs the dump.json content
- `command-5`: `cat *-dump.html` -- outputs the dump.html content

### `agent/forge-output.txt`
The raw stdout from the forge CLI execution (tee'd from command-1).

## `verifier/` directory

### `verifier/reward.txt`
Single number: `0` (fail) or `1` (pass).

### `verifier/ctrf.json`
CTRF (Common Test Report Format) output. Structure:
```json
{
  "results": {
    "tool": { "name": "pytest", "version": "..." },
    "summary": { "tests": N, "passed": N, "failed": N, ... },
    "tests": [
      {
        "name": "test_outputs.py::test_name",
        "status": "passed" | "failed",
        "trace": "full assertion traceback...",
        "message": "summary of failure"
      }
    ]
  }
}
```
The `trace` field is critical -- it contains the exact assertion that failed, the expected values, and often reveals hidden file paths, secret values, or commit SHAs the agent needed to handle.

### `verifier/test-stdout.txt`
Full pytest output including setup, test results, and detailed failure traces. Often more readable than ctrf.json for understanding assertion failures.

## dump.json structure

The conversation dump is a JSON array of messages. Each message has:
- `role`: "system", "user", or "assistant"
- `content`: text content or tool call results

Tool calls appear as structured objects within assistant messages:
- `name`: tool name (e.g., "fs_search", "patch", "shell")
- `input`: tool parameters
- Tool results appear in subsequent messages with the tool output

### Navigating dump.json efficiently

For large dumps (>1MB), use `fs_search` within the dump file rather than reading it entirely:
- Search for tool names: `"name": "fs_search"`, `"name": "patch"`, `"name": "write"`
- Search for specific file paths from verifier expectations
- Search for secret/token values the agent should have found
- Search for truncation warnings: `"truncated"`, `"INCOMPLETE"`
- Search for the agent's final summary/conclusion
