---
name: resolve-fixme
description: Find all FIXME comments across the codebase and attempt to resolve them. Use when the user asks to fix, resolve, or address FIXME comments, or when running the "fixme" command. Runs a script to locate every FIXME with surrounding context (2 lines before, 5 lines after) and then works through each one systematically.
---

# Resolve FIXME Comments

## Workflow

### 1. Run the discovery script

Execute the script from the repository root to collect all FIXMEs with context:

```
bash .forge/skills/resolve-fixme/scripts/find-fixme.sh [PATH]
```

- `PATH` is optional; omit it to search the entire working directory.
- The script prints each FIXME with **2 lines of context before** and **5 lines after**, along with the exact file path and line number.
- Skips `.git/`, `target/`, `node_modules/`, and `vendor/`.
- Requires either `rg` (ripgrep) or `grep` + `python3`.

### 2. Triage the results

Read the script output and build a work list. For each FIXME note:
- The file and line number (shown in the header of each block).
- The surrounding context to understand what the FIXME is asking for.
- Whether the fix requires code changes, further research, or is blocked.

### 3. Resolve each FIXME

Work through the list one at a time:

1. Read the full file section to understand the intent.
2. Implement the fix — edit the code, add the missing logic, or refactor as needed.
3. Remove the FIXME comment once the issue is resolved.
4. If a FIXME cannot be safely resolved (e.g. requires external input or is intentionally deferred), leave it in place and note why.

### 4. Verify

After resolving all FIXMEs, run the project's standard verification steps:

```
cargo insta test --accept
```

Re-run the discovery script to confirm no FIXMEs remain unresolved.

## Notes

- Prefer targeted, minimal fixes — only change what the FIXME describes.
- If the FIXME comment describes a TODO that was intentionally deferred (e.g. `FIXME(later):` or `FIXME(blocked):`), skip it and report it to the user.
- When the context is ambiguous, read more of the surrounding file before making a change.
