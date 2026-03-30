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

Run verification immediately. Do not do extra planning.

## Step 1 — Use the injected matrix

If the transcript contains a `<verification-matrix>` inside the system reminder, treat it as the checklist.
Do not rebuild the matrix yourself.

If no matrix is present:
1. find the real verifier quickly (`/app/test*`, `/app/check*`, `/app/verify*`, `/app/grade*`)
2. otherwise infer the exact invocation from the prompt
3. if neither exists, write a tiny smoke test for the critical output path

## Step 2 — Run the checks

Run the real verifier or smoke test first.
Then run only the extra commands needed to cover checklist items the verifier does not already prove.

Typical checklist items:
- exact artifact/path exists at the final verifier-inspected location, including after cleanup
- exact interface/CLI/import/endpoint works with runtime dependencies actually available
- bounded ranges are fully covered when small (for example `2..10` means every value)
- thresholds are measured explicitly when the task has speed, accuracy, size, similarity, loss, or other numeric pass/fail targets
- non-trivial/default path is tested, not only invalid or trivial cases
- held-out/sample-2 coverage is checked when one example can be overfit
- hidden constraint files and verifier-adjacent scripts are actually read when they exist
- untouched content remains byte-identical when the task only allows surgical edits
- oracle/reference comparison is used when exact outputs, legal moves, recovered values, or fitted parameters must match
- fresh-state / cleanup expectations still hold after verification
- live service, VM, multi-process, signal, or distributed behavior is verified with the real runtime path, not a syntax-only check

Leave the actual command output in the transcript.

## Step 3 — Final cleanup

Remove temporary test scripts and scratch artifacts you created.
Do not delete deliverables or runtime dependencies.
If final verification created extra artifacts in the deliverable directory, clean them before finishing.

## What NOT To Do

- Do NOT spend turns making tables or rewriting the matrix.
- Do NOT treat invoking this skill by itself as verification.
- Do NOT stop at syntax-only checks when runtime behavior is part of the task.
- Do NOT test only sampled points when the task gives a small bounded range.
- Do NOT assume a file that existed earlier still exists after cleanup; re-check final state.
- Do NOT validate only a single example when the task can overfit to one sample or one environment.
- Do NOT leave extra verification artifacts in the final workspace.
