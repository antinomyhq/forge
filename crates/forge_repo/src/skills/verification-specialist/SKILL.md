---
name: verification-specialist
description: MANDATORY verification workflow that MUST be invoked before marking ANY task as complete, especially in background/automated environments. Proactively call this skill when you have finished implementing a solution — do NOT wait for the user to ask. Covers running tests, checking builds, validating outputs, and ensuring robustness. Skipping this skill and marking a task complete without verification is a critical failure.
---

# Verification Specialist

This skill provides a systematic approach to verifying your work, ensuring it is robust, and properly completing tasks.

## Core Principles

1. **Reconstruct Requirements from Context**: You have the full conversation history. Use it — scroll back to the original user message that defined this task and extract requirements from it.
2. **Verify Functional Correctness**: Compilation or parsing success does NOT mean the solution works. Always run functional tests.
3. **Traceability**: Every requirement must map to at least one concrete verification command. If a requirement has no runnable test, it is not verified.
4. **Minimal State Changes**: Only modify files necessary to satisfy the requirements. Do not leave behind temporary scripts or modified configurations unless requested.

---

## Step 1 — Requirements Extraction from Conversation History

You are invoked after implementation. Your first job is to reconstruct what was asked.

**Go back through the conversation history and find:**
1. The original user message that described the task.
2. Any follow-up clarifications or scope changes from the user.
3. Any implicit requirements (e.g., "don't break existing behavior", "follow the same pattern as X").

From this, produce a Requirements Matrix — one row per discrete, independently testable behavior. Do not group multiple behaviors into one row.

**Format:**

```
| # | Requirement                              | Source                  | How to Verify                                      | Status  |
|---|------------------------------------------|-------------------------|----------------------------------------------------|---------|
| 1 | <exact behavior expected>                | user message / implicit | <runnable command that proves it works>             | pending |
| 2 | <exact behavior expected>                | user message / implicit | <runnable command that proves it works>             | pending |
```

**Rules:**
- **Source** must be one of: `user message`, `implicit` (unstated but obviously required), or `follow-up` (from a later clarification).
- **How to Verify** must be a concrete, runnable shell command, test name, or observable output — never "review code" or "check manually".
- Every implicit requirement (e.g., no regressions, code compiles, existing tests still pass) must appear as an explicit row.

**Example** (for a task: "create a Gemini reasoning effort transformer, High for first 10 messages, Medium for next 40, High again after 50"):

```
| # | Requirement                                   | Source        | How to Verify                                          | Status  |
|---|-----------------------------------------------|---------------|--------------------------------------------------------|---------|
| 1 | High effort when < 10 assistant messages      | user message  | cargo test test_reasoning_effort_high_for_first_10     | pending |
| 2 | Medium effort for messages 10–49              | user message  | cargo test test_reasoning_effort_medium_for_10_to_49   | pending |
| 3 | High effort again at 50+ messages             | user message  | cargo test test_reasoning_effort_high_for_50_and_above | pending |
| 4 | No-op when thinking_config is absent          | implicit      | cargo test test_reasoning_effort_noop_without_thinking | pending |
| 5 | Transformer is wired into the pipeline        | implicit      | grep -n "ReasoningEffort" pipeline.rs                  | pending |
| 6 | Existing tests still pass                     | implicit      | cargo test -p forge_app                                | pending |
```

---

## Step 2 — Verification Execution

For each row in the Requirements Matrix, run the exact verification command and record the result. All rows must reach `verified` status.

### Testing Rules
- Run tests using the `shell` tool — never assume they pass.
- If a test does not exist for a requirement, write it first, then run it.
- Test edge cases: empty inputs, boundary values (e.g., exactly at the threshold), and maximum values.
- If outputs have a required format (JSON, CSV, etc.), parse and validate the output, do not just check it is non-empty.

### Build & Lint
- Run `cargo check` (or equivalent) to confirm no compilation errors.
- Run `cargo clippy` (or `npm run lint`, etc.) to confirm no lint regressions.
- These must pass even if the task did not explicitly ask for it.

---

## Step 3 — Cleanup & Final Audit

1. **Remove temporary artifacts**: Delete any test scripts or files created solely for verification using the `remove` tool.
2. **Search for missed cases**: If the task involved replacing or removing all occurrences of something, run `fs_search` to confirm no instances remain.
3. **Final Requirements Matrix review**: Every row must be `verified`. Any row that is not `verified` means the task is incomplete.

---

## Completed Requirements Matrix (Final State)

Before closing the task, output the final Requirements Matrix with all statuses set to `verified`. This serves as the completion proof.

```
| # | Requirement                                      | How to Verify                                             | Status   |
|---|--------------------------------------------------|-----------------------------------------------------------|----------|
| 1 | ...                                              | ...                                                       | verified |
```

---

## Common Failure Patterns to Avoid

- **Skipping Requirements Extraction**: Diving into code without listing requirements first leads to missed edge cases and incomplete implementations.
- **Vague Verification Commands**: "Check the output looks right" is not a verification command. Use executable commands with observable pass/fail results.
- **Incomplete Solutions**: Partial implementations assumed to work. Every requirement must be fully implemented and verified.
- **Compound Requirements**: Bundling multiple behaviors into one row makes it impossible to tell which part failed.
- **Test Script Cleanup**: Forgetting to remove temporary test scripts after verification.
- **Assuming Tests Pass**: Never mark a requirement `verified` without running its test command.

---

## When to Re-plan

If verification fails more than 3 times for the same requirement, stop and redesign the approach. Add a new row to the Requirements Matrix capturing what changed, and re-verify from scratch. Incremental fixes on a broken approach compound errors.
