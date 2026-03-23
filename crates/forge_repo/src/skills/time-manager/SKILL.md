---
name: time-manager
description: >-
  Time budgeting and checkpoint strategy for time-constrained tasks.
  Use when session_elapsed_secs shows >40% of expected budget consumed,
  or when the task involves iterative compilation/testing cycles that
  risk timeout. Do NOT invoke at the start of a task — start working
  first, then invoke this skill when you notice time pressure.
  Provides phase-based budgeting, early artifact checkpointing, and
  graceful degradation strategies.
---

# Time Manager

Strategy for completing tasks within a fixed time budget without losing partial progress.

## Phase Budget

Read `session_elapsed_secs` from the `<session_info>` element appended to every tool output. The total budget is the `task_timeout_secs` value rendered in your system prompt (if present); otherwise estimate from task complexity (default: ~15 minutes). Divide into four phases:

| Phase | Budget | Goal |
|-------|--------|------|
| 1. Explore | 0-10% | Read task, `ls` directory, identify constraints, run runtime gate, read any provided checker/test scripts |
| 2. Implement | 10-70% | Write code, compile, iterate on basic correctness. **Save the first working artifact to its final path as early as possible.** |
| 3. Harden | 70-90% | Edge cases, optimization, ALL parameter values, constraint verification |
| 4. Verify & Clean | 90-100% | Final verification-specialist check, workspace cleanup, ensure only requested files remain |

## Checkpoint Protocol

Once Phase 2 produces a working (even partial) solution:

1. **Save immediately** to the expected output path (e.g., `/app/solution.c`, `/app/data.comp`).
2. Run any available checker/verifier to confirm at least partial correctness.
3. **Never delete a working artifact to attempt a risky rewrite.** Copy to `.backup` first:
   ```
   cp /app/solution.c /app/solution.c.backup
   ```
4. If the rewrite fails or time runs short, restore from backup.
5. Continue improving only via safe overwrites that preserve prior correctness.

## Timeout Escape Hatch

When elapsed time exceeds **85%** of your estimated budget:

1. **Stop all optimization and exploration immediately.**
2. Ensure the best-known artifact is at the expected output path.
3. If a backup exists and the current version is broken, restore from backup.
4. Run workspace cleanup (remove temp files, test scripts, compilation artifacts).
5. Exit gracefully. A partial solution that passes some checks is better than no output.

## Time Check Discipline

- After every `shell` command, glance at `session_elapsed_secs` in the `<session_info>` tag.
- Log your current phase mentally. If you're behind schedule, skip to the next phase.
- If you've spent >30% of total budget on a single sub-problem without progress, abandon that approach and try an alternative.
- For tasks with slow operations (compilation, model download, large installs), start them in the background immediately and work on other parts while waiting.

## When to Invoke This Skill

- At the start of any task you estimate will take >5 minutes
- When `session_elapsed_secs` exceeds 40% of your estimated budget and core artifacts don't exist yet
- When you're about to attempt a risky rewrite of working code
- When a compilation/test cycle has failed 3+ times and significant time has passed
