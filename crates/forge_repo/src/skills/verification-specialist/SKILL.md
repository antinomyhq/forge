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

Run verification immediately.

1. If the transcript contains a `<verification-matrix>` in the system reminder, use it as the checklist. Do not rebuild it.
2. Run the real verifier first. If none exists, infer the exact command from the task. If that is impossible, run a tiny smoke test for the critical output path.
3. Run only the extra checks needed to cover matrix items the verifier does not already prove.
4. Leave command output in the transcript.
5. Clean up temporary verification artifacts, then re-check final deliverable paths if cleanup could affect them.

Focus on a few things only:
- exact final artifact/path
- exact runtime interface
- hidden constraints or constrained diffs
- full coverage of small bounded ranges
- explicit numeric thresholds
- held-out/default-path coverage when one sample can overfit
- real runtime behavior for services, VMs, signals, or distributed code

Do not:
- treat invoking this skill as verification
- stop at syntax-only checks when runtime matters
- sample a small bounded range
- leave extra verification artifacts in the workspace
