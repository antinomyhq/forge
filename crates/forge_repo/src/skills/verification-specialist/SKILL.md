---
name: verification-specialist
description: MANDATORY verification workflow that MUST be invoked before marking ANY task as complete, especially in background/automated environments. Proactively call this skill when you have finished implementing a solution — do NOT wait for the user to ask. Covers running tests, checking builds, validating outputs, and ensuring robustness. Skipping this skill and marking a task complete without verification is a critical failure.
---

# Verification Specialist

This skill provides a systematic approach to verifying your work, ensuring it is robust, and properly completing tasks.

## Core Principles

1. **Verify Functional Correctness**: Compilation or parsing success does NOT mean the solution works. Always run functional tests.
2. **Minimal State Changes**: Only modify files necessary to satisfy the requirements. Do not leave behind temporary scripts or modified configurations unless requested.
3. **Comprehensive Testing**: Test edge cases, boundary conditions, and verify exact output formats.

## Verification Checklist

Before marking a task as `completed` using the `todo_write` tool, you MUST complete these steps:

### 1. Requirements Review
- Re-read the original task instructions.
- Ensure every single requirement has been addressed.
- If the task says "remove X and replace with Y", ensure you have done BOTH.

### 2. Testing & Validation
- **Write Tests**: If the project has a test suite, write tests for your new code. If it's a standalone script, write a temporary verification script.
- **Run Tests**: Execute the tests using the `shell` tool and confirm they pass. 
  - Test each requirement separately.
  - Test edge cases (empty inputs, maximum values, off-by-one errors).
  - Test with multiple different inputs to verify generalization.
- **Format Verification**: If an output format is specified (e.g., JSON, CSV), validate that the output can be parsed correctly.

### 3. Cleanup & Sanitization
- Remove any temporary test scripts or files you created for verification (unless explicitly required by the task) using the `remove` tool.
- If you were asked to remove/replace ALL occurrences of something, do a final `fs_search` to ensure no instances were missed.

### 4. Build & Lint (If Applicable)
- Run the project's build command (e.g., `cargo check`, `npm run build`) to ensure there are no compilation errors.
- Run the project's linter (e.g., `cargo clippy`, `npm run lint`) to ensure code quality.

## Common Failure Patterns to Avoid

- **Incomplete Solutions**: Creating partial implementations and assuming they work. Every requirement must be fully implemented and tested.
- **Insufficient Testing**: Writing tests that pass but don't cover edge cases or match exact requirements. Your tests must be rigorous.
- **External Dependencies**: Avoid adding unnecessary external libraries. Prefer standard library solutions or already installed dependencies unless explicitly requested.
- **Test Script Cleanup**: Forgetting to remove temporary test scripts.

## When to Re-plan

If your verification fails more than 3 times for the same issue, step back and redesign your approach. A fresh strategy is often better than incremental fixes to a broken approach.