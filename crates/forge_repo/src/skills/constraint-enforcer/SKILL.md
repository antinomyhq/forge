---
name: constraint-enforcer
description: >-
  Systematic constraint verification for tasks with explicit rules,
  size limits, allowed-edit restrictions, performance requirements,
  or anti-shortcut constraints. Invoke AFTER you have a working
  implementation, BEFORE declaring completion — never at the start
  of a task. Use when the task prompt contains words like "must",
  "only", "do not", "at most", "limit", size/performance bounds,
  or anti-shortcut rules.
---

# Constraint Enforcer

Extract and verify every constraint from the task prompt AFTER you have a working implementation.

**IMPORTANT**: This skill is for the VERIFICATION phase, not the planning phase.
Do NOT invoke this skill before you have written and tested your solution.
Implement first, then use this skill to check that all constraints are satisfied.

## Step 1 -- Extract Constraints

Re-read the original task prompt. Extract every constraint into a typed checklist:

```
| # | Type        | Constraint                          | Verification Command                    | Status  |
|---|-------------|-------------------------------------|-----------------------------------------|---------|
| 1 | SIZE        | output <= 2500 bytes                | wc -c /app/data.comp                    | pending |
| 2 | PERFORMANCE | faster than reference for ALL sizes | for s in 2 3 5 10; do bench $s; done    | pending |
| 3 | CONTENT     | only synonym substitutions allowed  | diff orig final | check_allowed_words   | pending |
| 4 | METHOD      | write real C code, not a wrapper    | file ./image && no calls to /app/orig   | pending |
| 5 | COVERAGE    | works for world_size 1,2,4          | for ws in 1 2 4; do test $ws; done      | pending |
| 6 | FORMAT      | output is valid JSON                | python3 -c "import json; json.load(...)"| pending |
| 7 | FRESHNESS   | service works for fresh client      | rm -rf /tmp/clone && git clone && test   | pending |
```

### Constraint Types

- **SIZE**: Output file must be <= N bytes/lines/tokens. Check with `wc -c`, `wc -l`, or `stat --format=%s`.
- **PERFORMANCE**: Must beat a baseline or run within N seconds. Test at ALL specified input sizes, not just the easiest. Small inputs often have different optimal strategies than large ones.
- **CONTENT**: Only specific edits are allowed (synonyms, allowed fields). Diff the original against the final and verify every changed token is in the allowed set.
- **METHOD**: Must write real code, not wrap existing binaries. Verify with `file`, `strace`, or by checking the solution works in an isolated directory without reference binaries.
- **COVERAGE**: Must work for ALL specified parameter values. world_size=1 is a degenerate case; 2x2 matrices differ from 100x100. Loop over every value in the spec.
- **FORMAT**: Output must match a specific schema. Parse it with the expected consumer, not just check non-empty.
- **FRESHNESS**: Verifiers always start from clean state. Clean all test artifacts, then verify with a completely new client/session.

## Step 2 -- Verify Each Constraint

For each row, run the verification command and record pass/fail.

Rules:
- **Every constraint gets its own runnable check.** "Looks correct" is not verification.
- **Test the boundary.** If the limit is 2500 bytes, ensure your output is safely under (not 2499).
- **Test ALL parameter values.** If the task says "sizes 2 to 10", test 2, 3, 4, ..., 10. If the task says "world_size 1, 2, 4", test all three.
- **Constraints are independent of correctness.** A passing test suite does NOT mean constraints are satisfied. You can have correct output that violates a size limit.

## Step 3 -- Anti-Shortcut Hardening

When the task says "write X", "implement X", or "create X":

1. **Self-containment check**: Does the solution work without ANY pre-existing task binaries? Would it work if you moved it to an empty directory?
2. **No binary wrapping**: Verify your solution doesn't shell out to reference binaries (`/app/orig`, `/app/decomp`, etc.) at runtime. Check with:
   ```
   grep -r "exec\|system\|popen\|subprocess\|/app/" your_solution
   ```
3. **Chroot test** (if feasible): Copy your binary to an empty directory and run it. If it fails because it depends on task-provided files, it's not self-contained.
4. **Read the verifier** (if discoverable): Some tasks include test scripts. Read them to understand what isolation they use (chroot, strace, etc.).

## Step 4 -- Remediation

If any constraint fails:
1. Do NOT mark the task complete.
2. Fix the specific violation.
3. Re-run ALL constraint checks (fixing one may break another).
4. Only proceed to verification-specialist after all constraints pass.
