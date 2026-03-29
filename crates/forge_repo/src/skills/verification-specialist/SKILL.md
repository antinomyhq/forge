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
4. **Leave evidence in the conversation**: before finishing, the transcript must contain the actual verification command output or smoke-test result, not just a claim that verification happened.

**Do NOT build a requirements matrix.** Just run the verifier. If there is no discoverable verifier, write a 5-line smoke test that exercises the critical output path.
If the task produces a binary, script, generated file, or service, verify that exact artifact directly (run the binary, invoke the script, inspect the generated file with the expected consumer, or hit the service from a fresh client).

## Step 2 — Constraint Quick-Check

Extract constraints from the task prompt and verify each with ONE command:

- **SIZE**: `wc -c output_file` or `stat --format=%s`
- **PERFORMANCE**: Run benchmark at ALL specified sizes (not just the easiest); if the domain is small and bounded (e.g. sizes 2..10), cover every value in the range
- **FORMAT**: Parse output with the expected consumer (`python3 -c "import json; json.load(open(...))"`)
- **METHOD**: Verify no calls to reference binaries: `grep -r "/app/orig\|subprocess.*reference" your_code`
- **COVERAGE**: Loop over ALL parameter values: `for ws in 1 2 4; do test $ws; done`
- **FRESHNESS**: Clean state, then test: `rm -rf /tmp/test && fresh_client_test`

Skip types that don't apply. Don't print tables — just run commands and report pass/fail.

## Step 3 — Sanity-Check Outputs

Before declaring complete, catch common silent failures:

1. **Numerical outputs**: Print the key values. Are they physically plausible? (Peak width shouldn't be 10x the fitting window. Speedup shouldn't be 0.5x. Eigenvalue shouldn't be NaN.)
2. **File outputs**: Check size is non-trivial for the task complexity. A 12-byte output for a compression task is wrong.
3. **Service outputs**: Hit the endpoint with a fresh client. Don't rely on cached state from your testing.
4. **Multi-parameter tasks**: Test the FULL parameter range, not just the trivial case. world_size=1 hides all sharding bugs. If a helper script only samples part of a bounded domain, add your own loop to cover the missing values before concluding.
5. **Build artifacts**: If compilation is part of the task, confirm the expected artifact path exists before declaring success. A green compile log without the required output file is not enough.

## Step 4 — Cleanup & Final Run

1. Remove your test scripts, temp files, compilation artifacts.
2. **Do NOT remove**: deployed content (webroot files, database records, git repos, config files, .so files the deliverable imports).
3. `ls` every output directory — only requested deliverables should remain.
4. **Run the verifier one final time** against the cleaned workspace. If that final run creates temporary binaries, build products, or scratch files, clean them again before finishing. The final visible workspace state must still match what the task expects.

## What NOT To Do

- Do NOT spend 3+ tool calls building ASCII tables. Run the checks directly.
- Do NOT write "review code" or "check manually" as a verification step. Every check must be a runnable command.
- Do NOT treat invoking this skill by itself as verification. The required verifier or smoke-test commands still need to be run.
- Do NOT test only the trivial case (world_size=1, 2x2 matrix, empty input) and declare success.
- Do NOT assume a helper benchmark or smoke test covers the full domain; if the prompt gives a small bounded range, explicitly enumerate it yourself.
- Do NOT leave test artifacts in the workspace. But also do NOT delete runtime dependencies the deliverable needs.
- Do NOT run your final verification in a way that leaves extra binaries or scratch outputs in the deliverable directory; either place them elsewhere or clean them before finishing.
- Do NOT accept curve-fit results without sanity-checking the fitted parameters are physically plausible.
