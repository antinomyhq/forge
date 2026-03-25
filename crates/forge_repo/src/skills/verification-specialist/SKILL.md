---
name: verification-specialist
description: >-
  MANDATORY verification workflow that MUST be invoked before marking
  ANY task as complete. Proactively call this skill when you have
  finished implementing a solution — do NOT wait for the user to ask.
  Focuses on running the actual verifier checks, not generating tables.
  Skipping this skill and marking a task complete without verification
  is a critical failure.
---

# Verification Specialist

Fast, targeted verification before completing a task. Skip ceremony — run checks.

## Step 1 — Reconstruct the Verifier

Re-read the task prompt. Figure out exactly how the verifier will test your output:

1. **Check for test scripts**: `ls /app/test* /app/check* /app/verify* /app/grade* 2>/dev/null` — if they exist, READ them.
2. **Infer from the prompt**: If the task says "write X that passes the tests", "the grader checks Y", or mentions import paths / CLI invocations / HTTP endpoints — reconstruct that exact invocation.
3. **Run it**: Execute the reconstructed verifier command. If it passes, you're likely done. If it fails, fix the failures.

**Do NOT build a requirements matrix.** Just run the verifier. If there is no discoverable verifier, write a 5-line smoke test that exercises the critical output path.

## Step 2 — Constraint Quick-Check

Extract constraints from the task prompt and verify each with ONE command:

- **SIZE**: `wc -c output_file` or `stat --format=%s`
- **ARTIFACT PATHS**: Verify every required deliverable exists at the exact path and filename from the prompt (`for p in /app/out1 /app/out2; do test -e "$p" || exit 1; done`)
- **ARTIFACT PARSEABILITY**: Open each deliverable with its real consumer (`python3 -c "import json; json.load(open('/app/results.json'))"`, `python3 -c "import pandas as pd; pd.read_csv('/app/output.csv')"`)
- **PERFORMANCE**: Run benchmark at ALL specified sizes (not just the easiest)
- **FORMAT**: Parse output with the expected consumer (`python3 -c "import json; json.load(open(...))"`)
- **METHOD**: Verify no calls to reference binaries: `grep -r "/app/orig\|subprocess.*reference" your_code`
- **COVERAGE**: Loop over ALL parameter values: `for ws in 1 2 4; do test $ws; done`
- **FRESHNESS**: Clean state, then test: `rm -rf /tmp/test && fresh_client_test`

Skip types that don't apply. Don't print tables — just run commands and report pass/fail.

## Step 3 — Sanity-Check Outputs

Before declaring complete, catch common silent failures:

1. **Numerical outputs**: Print the key values. Are they physically plausible? (Peak width shouldn't be 10x the fitting window. Speedup shouldn't be 0.5x. Eigenvalue shouldn't be NaN.)
2. **File outputs**: Check size is non-trivial for the task complexity. A 12-byte output for a compression task is wrong.
3. **Artifact contract**: Confirm expected filenames/paths/extensions exactly match the prompt and can be loaded by downstream tools.
4. **Service outputs**: Hit the endpoint with a fresh client. Don't rely on cached state from your testing.
5. **Multi-parameter tasks**: Test the FULL parameter range, not just the trivial case. world_size=1 hides all sharding bugs.

## Step 4 — Dependency Gate (Fail Closed)

Before declaring verification complete:

1. Confirm required runtime dependencies for behavioral checks are installed (for example `torch` for distributed PyTorch tasks).
2. If missing, install or recreate an environment where the verifier-equivalent checks can run.
3. If still unavailable, do NOT mark the task complete or claim full verification; report explicit blocker details (missing dependency, attempted commands, and which checks could not run).

## Step 5 — Cleanup & Final Run

1. Remove your test scripts, temp files, compilation artifacts.
2. **Do NOT remove**: deployed content (webroot files, database records, git repos, config files, .so files the deliverable imports).
3. `ls` every output directory — only requested deliverables should remain.
4. **Run the verifier one final time** against the cleaned workspace. This is the state the real verifier sees.

## What NOT To Do

- Do NOT spend 3+ tool calls building ASCII tables. Run the checks directly.
- Do NOT write "review code" or "check manually" as a verification step. Every check must be a runnable command.
- Do NOT test only the trivial case (world_size=1, 2x2 matrix, empty input) and declare success.
- Do NOT declare full verification complete when required runtime dependencies are missing.
- Do NOT leave test artifacts in the workspace. But also do NOT delete runtime dependencies the deliverable needs.
- Do NOT accept curve-fit results without sanity-checking the fitted parameters are physically plausible.
