---
name: write-evals
description: "Create and iterate evaluation benchmarks under benchmarks/evals from a user-provided problem statement. Use when a user asks to add or improve eval coverage, wants benchmark-first development, or asks to validate a behavior with a failing benchmark before optimization. This skill enforces a red-green loop: author a failing eval first, confirm it fails, then optimize/build/retest until the eval passes."
---

# Write Evals

Implement benchmark-first eval development in `benchmarks/evals`.

## Core Workflow

1. Translate the user's problem statement into a **single measurable eval objective**.
2. Create a new eval directory at `benchmarks/evals/<eval_name>/`.
3. Add the minimal evaluation inputs (`task.yml` and data source files such as CSV) so the first run targets the requested behavior.
4. Run the eval and confirm it **fails first**.
5. If failure is not observed, tighten validations or inputs until the eval fails for the right reason.
6. Optimize/fix implementation.
7. Build and retest.
8. Repeat steps 6-7 until the eval passes consistently.

Never skip the initial failing run.

## Eval Authoring Rules

- Keep scope narrow: one eval should validate one behavioral objective.
- Use deterministic validations whenever possible (`regex` or `shell`).
- Prefer clear task data columns that map directly to placeholders used in `task.yml`.
- Keep runtime practical with explicit `timeout` and appropriate `parallelism`.
- Set `early_exit: true` when validations are sufficient to conclude pass/fail.

## Required Files and Structure

Create:

- `benchmarks/evals/<eval_name>/task.yml`
- One or more source files referenced by `sources` (typically `*.csv`)

Use repository conventions already present under `benchmarks/evals/*/task.yml`.

## `task.yml` Baseline Pattern

Use this as the starting pattern and adapt to the problem statement:

```yaml
run:
  - FORGE_DEBUG_REQUESTS='dir/context.json' forgee -p 'task'
parallelism: 1
timeout: 120
early_exit: true
validations:
  - name: "Describe expected behavior"
    type: shell
    command: "jq -e '<your condition>' dir/context.json"
sources:
  - csv: tasks.csv
```

If setup is required before each run, add `before_run` commands.

## Red-Green Optimization Loop

### Red Phase (must happen first)

1. Run the new eval:
   ```bash
   npm run eval ./evals/<eval_name>/task.yml
   ```
2. Confirm a failing outcome (`validation_failed`, `failed`, or `timeout` depending on target).
3. Inspect debug artifacts and logs under:
   `benchmarks/evals/<eval_name>/debug/<timestamp>/`
4. Verify failure is caused by the intended unmet behavior.

If it passes immediately, improve task data or validations until it fails for the intended reason.

### Green Phase

1. Implement optimization/fix in codebase.
2. Build in debug mode:
   ```bash
   cargo build
   ```
3. Re-run the eval:
   ```bash
   npm run eval ./evals/<eval_name>/task.yml
   ```
4. Continue until the eval passes.

Do not use release builds for this loop.

## Validation Design Guidance

- For tool-usage checks, prefer parsing `context.json` with `jq`.
- For output-content checks, use robust regex patterns that avoid overfitting timestamps/format noise.
- For multi-step behavior, use multiple validation entries with explicit names.
- Keep validations strict enough to catch regressions, but not brittle to harmless formatting differences.

## Completion Criteria

Consider the eval complete only when all conditions are true:

1. Eval files are added in `benchmarks/evals/<eval_name>/`.
2. Initial run was observed failing.
3. Optimization/build/retest loop executed.
4. Latest run passes with expected validations.
5. The eval is reusable and understandable from its `task.yml` + data files alone.
