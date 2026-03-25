---
name: constraint-enforcer
description: >-
  Extract hidden constraints from task prompts that cause silent
  verification failures. Invoke ONCE at the start of a task — before
  writing any code — when the prompt contains words like "must",
  "only", "do not", "at most", "limit", size/performance bounds,
  anti-shortcut rules, or specifies exact output formats. Produces
  a compact checklist of constraints with verification commands.
  Do NOT invoke this skill at the end of a task or during verification.
---

# Constraint Enforcer

Extract constraints BEFORE coding. This skill runs once at task start, not at the end.

## What To Do

Re-read the task prompt carefully. Extract every constraint into a **short** checklist — one line per constraint with a verification command. Focus on constraints that are **easy to miss**:

### Hidden constraint patterns (these cause most failures):

1. **Output format**: Does the task specify JSON, CSV, a specific schema, exact field names, or a file path? Verifiers parse the output — wrong field names = 0 points.
2. **Size limits**: "at most N bytes", "under N lines", "compressed size <= N". Check with `wc -c`.
3. **Parameter coverage**: "works for world_size 1,2,4" means test ALL three, not just 1. "matrix sizes up to 100x100" means test 100x100 specifically.
4. **Anti-shortcut rules**: "write real code, not a wrapper", "implement from scratch", "no external libraries". Verifiers use strace/chroot to enforce these.
5. **Exact tool requirements**: "use tool X to compute Y" means install X, don't use a substitute.
6. **Implicit freshness**: Service tasks are tested from a clean client. Your test cookies/state don't carry over.
7. **Taxonomy specificity**: "classify using CWE IDs" means pick the most specific CWE, not the generic parent.
8. **Byte-identical preservation**: "only change X" means everything else must be byte-identical. Use surgical edits. Add a scope check command like `git diff --name-only` and verify only allowed paths appear.

### Output format

Print your checklist as a compact list — NOT a markdown table. Example:

```
CONSTRAINTS:
1. SIZE: output <= 2500 bytes → wc -c /app/data.comp
2. COVERAGE: world_size 1,2,4 → for ws in 1 2 4; do test.py --ws=$ws; done
3. FORMAT: valid JSON with keys "G" and "2D" → python3 -c "import json; d=json.load(open('/app/results.json')); assert set(d)=={'G','2D'}"
4. METHOD: no wrapping /app/orig → grep -r '/app/orig' my_code (must return empty)
5. SCOPE: only allowed files changed → git diff --name-only | sort
```

Then proceed to implementation. Verify each constraint AFTER the first working artifact is ready — not before.

## What NOT To Do

- Do NOT invoke this skill at the end of a task. It's for planning, not verification.
- Do NOT build elaborate multi-column markdown tables. A numbered list is sufficient.
- Do NOT repeat constraints already obvious from the task (e.g., "produce the output file" is not a hidden constraint).
- Do NOT block implementation on constraint analysis — extract constraints in under 30 seconds, then start coding.
