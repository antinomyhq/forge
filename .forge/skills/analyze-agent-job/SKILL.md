---
name: analyze-agent-job
description: >-
  Deep analysis of agent benchmark job results to identify gaps in agent implementation.
  Use when the user provides a job results folder path (e.g., jobs/task-name__randomId/)
  containing agent execution artifacts, verifier test outputs, and conversation dumps.
  Performs forensic analysis of what the agent did, what it missed, why it failed,
  and produces actionable recommendations for prompt/tool improvements.
  Triggers: "analyze job", "why did agent fail", "check job results", "agent gap analysis",
  "analyze dump", "what went wrong", "agent performance analysis", providing a job folder path.
---

# Analyze Agent Job

Perform deep forensic analysis of agent benchmark job results to identify gaps in agent behavior, prompt design, and tool usage.

## Remote job folders

Job results may be on a remote evaluation machine. The user will provide the job folder name (e.g., `adaptive-rejection-sampler__3ywpX2e`).

**Remote host**: `ubuntu@34.63.88.42` via SSH key `~/.ssh/gcp_forge_eval`
**Base path on remote**: `~/forge/jobs/`

**Read-only access** — never modify, delete, or write anything on the remote machine.

**How to read remote files**: Use `ssh` via the shell tool to read files in place. Do NOT scp the entire folder locally. Read individual files as needed:

```sh
# List job folder contents
ssh -i ~/.ssh/gcp_forge_eval -o StrictHostKeyChecking=no ubuntu@34.63.88.42 "find ~/forge/jobs/<folder> -type f | sort"

# Read a specific file
ssh -i ~/.ssh/gcp_forge_eval -o StrictHostKeyChecking=no ubuntu@34.63.88.42 "cat ~/forge/jobs/<folder>/verifier/ctrf.json"

# Search within a file
ssh -i ~/.ssh/gcp_forge_eval -o StrictHostKeyChecking=no ubuntu@34.63.88.42 "grep -n 'pattern' ~/forge/jobs/<folder>/agent/command-1/stdout.txt"

# Read a large file partially (e.g., first/last N lines)
ssh -i ~/.ssh/gcp_forge_eval -o StrictHostKeyChecking=no ubuntu@34.63.88.42 "head -100 ~/forge/jobs/<folder>/agent/command-4/dump.json"
ssh -i ~/.ssh/gcp_forge_eval -o StrictHostKeyChecking=no ubuntu@34.63.88.42 "wc -l ~/forge/jobs/<folder>/agent/command-4/dump.json"
```

Replace `<folder>` with the user-provided folder name. Follow the same Phase 1-5 workflow below, substituting local `read` calls with `ssh cat` commands.

## Workflow

### Phase 1: Inventory the job folder

Read the job folder structure. Reference [job-folder-structure.md](references/job-folder-structure.md) for the expected layout and file formats.

Key files to read in order:
1. `config.json` -- task metadata, agent config, source benchmark
2. `result.json` -- outcome summary, reward, timing, exception info
3. `verifier/reward.txt` -- numeric reward (0 = fail, 1 = pass)
4. `verifier/ctrf.json` -- individual test results with failure traces
5. `verifier/test-stdout.txt` -- full verifier test output with assertion details
6. `agent/command-1/command.txt` -- the user prompt given to the agent
7. `agent/command-4/dump.json` -- the full conversation dump (primary analysis target)
8. `agent/command-1/stdout.txt` -- raw agent output log (if dump.json is unavailable)

If `reward.txt` shows 1.0, the agent passed -- analysis shifts to efficiency/quality review rather than failure diagnosis.

### Phase 2: Extract failure specifics from verifier

From `ctrf.json` and `test-stdout.txt`, extract:
- Which tests failed and their exact assertion messages
- What the verifier expected vs what it found
- Hidden assumptions (specific file paths, commit SHAs, exact replacement values, calling conventions)

Build a **verifier expectation map**: a concrete list of what the agent needed to produce to pass.

**Important**: For each expectation, immediately classify it:
- **Explicit**: clearly stated in the user prompt the agent received
- **Conventional**: follows well-known language/domain conventions the agent should know or look up
- **Hidden**: not in the prompt and not inferable from conventions -- this is a verifier assumption the agent could not have known

### Phase 3: Reconstruct the agent conversation

From `dump.json`, reconstruct the agent's decision trace. The dump contains the full message history with tool calls and results. Analyze:

1. **Tool call sequence** -- what tools were called, in what order, with what parameters
2. **Discovery coverage** -- which files/patterns did the agent search for vs what the verifier expected
3. **Truncation handling** -- did any search results get truncated? Did the agent follow up?
4. **Decision points** -- where did the agent narrow its hypothesis? Was the narrowing justified?
5. **Destructive actions** -- did the agent perform irreversible operations (git history rewrite, file deletion) without explicit request?
6. **Delegation patterns** -- if the agent delegated to sub-agents, was the delegation scoped correctly?
7. **Verification loop** -- did the agent verify its work before declaring completion?

### Phase 4: Run gap analysis

Read [analysis-dimensions.md](references/analysis-dimensions.md) for the full checklist. Produce findings for each dimension:

1. **Discovery gaps** -- things the verifier expected that the agent never found
2. **Tool usage errors** -- wrong tool for the job, missing tool calls, ignored tool output
3. **Search strategy failures** -- truncated results treated as complete, insufficient follow-up, narrow patterns
4. **Over-action** -- destructive changes beyond what was requested
5. **Under-action** -- things the agent could have done but didn't
6. **Prompt conflicts** -- tensions in the system prompt / partials that confused the agent
7. **Capability gaps** -- would a different/additional tool have helped?
8. **Verifier misalignment** -- hidden verifier assumptions the agent couldn't have known

### Phase 5: Produce the report

Structure the output as:

```
## Job Summary
- Task: {task name}
- Reward: {0.0 or 1.0}
- Tests: {passed}/{total}
- Duration: {agent execution time}

## Failure Analysis (if reward < 1.0)
For each failed test:
- What the verifier expected
- What the agent produced
- Root cause

## Agent Behavior Trace
- Key decision points with file:line references into dump.json
- Tool call summary (counts by tool type)
- Files discovered vs files the verifier expected

## Gap Analysis
For each applicable dimension from Phase 4:
- Finding
- Evidence (with file:line references)
- Severity (critical / moderate / low)

## Recommendations
Non-biasing, generic improvements that would help without being task-specific:
- Prompt changes
- Tool behavior changes
- Search strategy improvements
```

## Critical constraint: Agent cannot see verifier tests

The agent being analyzed **never has access to the verifier tests**. The verifier runs in a separate container after the agent finishes. The agent only sees:
- The user prompt (task description)
- The files in its working directory (e.g., `/app`)
- Whatever it discovers through its own tools (shell, search, read, fetch)

This means:
- **Do NOT recommend "inspect the verifier"** as a fix -- the agent literally cannot do this
- **Do NOT blame the agent for not matching hidden test expectations** unless the user prompt or discoverable files contained sufficient information to infer those expectations
- **DO focus on**: whether the agent could have reasonably inferred the correct behavior from the user prompt, common conventions, documentation, or API design best practices
- **The gap question becomes**: given ONLY the user prompt and the agent's available tools/knowledge, what should the agent have done differently to produce output that would satisfy reasonable external callers?

When analyzing failures, always ask:
1. Was the requirement explicit in the user prompt? If yes, the agent should have followed it.
2. Was the requirement implicit but inferable from common conventions? If yes, note the convention the agent missed.
3. Was the requirement hidden and not reasonably discoverable? If yes, classify as **verifier misalignment** (hidden assumption), not an agent gap.

## Important guidelines

- Always cite evidence using `filepath:lineNumber` or `filepath:startLine-endLine` format
- Focus on **generic** recommendations that improve the agent across tasks, not fixes biased toward this specific benchmark
- When analyzing prompt conflicts, read the actual prompt files: `crates/forge_repo/src/agents/forge.md`, `crates/forge_repo/src/agents/explorer.md`, `templates/forge-partial-background-instructions.md`, `templates/forge-partial-system-info.md`
- The dump.json conversation structure contains messages with `role` (system/user/assistant) and tool calls with inputs/outputs -- navigate it by searching for tool names, file paths, and key values from the verifier expectations
- When the dump.json is very large, use targeted searches (fs_search) within it rather than reading the entire file
