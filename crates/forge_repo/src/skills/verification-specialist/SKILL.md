---
name: verification-specialist
description: MANDATORY verification workflow that MUST be invoked before marking ANY task as complete, especially in background/automated environments. Proactively call this skill when you have finished implementing a solution — do NOT wait for the user to ask. Covers running tests, checking builds, validating outputs, and ensuring robustness. Skipping this skill and marking a task complete without verification is a critical failure.
---

# Verification Specialist

Systematic workflow for verifying work before completing a task.

## Core Principles

1. **Reconstruct Requirements**: Review the conversation history — original task, follow-ups, and implicit requirements (no regressions, code compiles, existing tests pass).
2. **Functional Correctness**: Compilation success does NOT mean the solution works. Always run functional tests.
3. **Traceability**: Every requirement must map to a concrete, runnable verification command. No command = not verified.
4. **Hard Completion Gate**: Completion is forbidden unless all requirements are verified, required artifacts exist, and output format is valid. Invoking this skill alone is not sufficient.

---

## Step 1 — Build Requirements Matrix

Extract every discrete, testable behavior from the conversation. One row per behavior — do not group multiple behaviors together. Include implicit requirements (no regressions, compiles, existing tests pass) as explicit rows.

```
| # | Requirement                              | Source                       | How to Verify                          | Status  |
|---|------------------------------------------|------------------------------|----------------------------------------|---------|
| 1 | <exact behavior expected>                | user message / implicit / follow-up | <runnable command or test name>   | pending |
```

**"How to Verify" must be a runnable shell command, test name, or observable output — never "review code" or "check manually".**

Example:
```
| # | Requirement                              | Source       | How to Verify                                        | Status  |
|---|------------------------------------------|--------------|------------------------------------------------------|---------|
| 1 | High effort when < 10 messages           | user message | cargo test test_effort_high_first_10                 | pending |
| 2 | Transformer wired into pipeline          | implicit     | grep -n "ReasoningEffort" pipeline.rs                | pending |
| 3 | Existing tests still pass                | implicit     | cargo test -p forge_app                              | pending |
```

---

## Step 2 — Verification Execution

Run each verification command and record the result. All rows must reach `verified`.

- Run tests via `shell` — never assume they pass.
- If no test exists for a requirement, write one first.
- Test edge cases: empty inputs, boundary values, max values.
- Validate output format (JSON, CSV, etc.) by parsing, not just checking non-empty.
- Run `cargo check` / `cargo clippy` (or equivalent lint) even if not explicitly requested.
- If verification fails 3+ times for the same requirement, stop and redesign the approach.

---

## Step 3 — Cleanup & Final Audit

1. If the task involved replacing/removing all occurrences, run `fs_search` to confirm none remain.
2. Confirm required artifacts exist and output format matches the task contract.
3. Every row in the Requirements Matrix must be `verified`. Any non-verified row means the task is incomplete.

Output the final matrix as completion proof.

---

## Failure Patterns to Avoid

- **Vague Verification**: "Check the output looks right" is not verification. Use executable commands with pass/fail results.
- **Compound Requirements**: One row per behavior. Bundling makes it impossible to tell which part failed.
- **Unsafe Cleanup**: Do not delete harness files, checker scripts, or required outputs. Do remove assistant-created temp scripts.
- **Handler Invocation != Correctness**: Verifying a handler was *entered* is not verifying it *completed correctly*. Assert the handler's outcome (side effects, return value, final state).
- **Rationalizing Weak Tests**: A test that passes despite broken implementation provides no safety. Redesign it to match the real execution context.
