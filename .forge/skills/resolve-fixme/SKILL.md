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

Read the script output and build a work list. For each FIXME:

**Collect the full comment first.** A FIXME may span multiple lines. The discovery script only shows the line where `FIXME` appears, but the actual instruction often continues on the lines immediately below it as additional comment lines. Before interpreting any FIXME, read forward from the FIXME line until the comment block ends — treat all consecutive comment lines as part of the same instruction. Do not act on a partial reading.

**Group related FIXMEs across files.** The same underlying task is often described by FIXMEs spread across multiple files — each one expressing a different facet of the same change (e.g. one file describes a new domain type to create, another describes a parameter to drop once that type exists, a third describes a service to build). Before planning any implementation, read all FIXMEs in full and identify which ones belong to the same task by looking for shared vocabulary, cross-references, or complementary instructions. Group these into a single consolidated task. Implement the task as a whole — do not fix one file in isolation if the FIXMEs describe a coordinated change.

For each individual FIXME (or group of related FIXMEs), record:

- The files, start lines, and end lines of all comment blocks in the group.
- A single consolidated description of the full implementation required — synthesising the intent from all comments in the group into one coherent plan.

Every FIXME must be resolved. There is no skip option.

### 3. Resolve every FIXME

Work through the list one at a time:

1. Read the full relevant section of the file.
2. Implement the fix fully — write the code, add the missing logic, refactor as needed. Do not stop short.
3. Remove the FIXME comment **only after** the implementation is complete and correct.

> **Critical rule:** Never delete or modify a FIXME comment without first completing the work it describes. A FIXME comment is the only record of what needs to be built. Removing it without doing the work silently destroys that record and is strictly forbidden.

If the implementation requires understanding other parts of the codebase first — read them. If it requires creating new types, files, or services — create them. Keep working until every FIXME is fully resolved.

### 4. Verify

After resolving all FIXMEs, run the project's standard verification steps:

```
cargo insta test --accept
```

Re-run the discovery script to confirm no FIXMEs remain.

## Notes

- Prefer targeted, minimal fixes — only change what the FIXME describes.
- When the context is ambiguous, read more of the surrounding file before making a change.
- If an implementation turns out to be larger than expected, break it into steps and work through them — do not use complexity as a reason to leave a FIXME unresolved.
