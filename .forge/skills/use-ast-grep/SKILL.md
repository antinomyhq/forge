---
name: use-ast-grep
description: Use ast-grep (sg) for structural code search, linting, and rewriting based on AST patterns rather than text. Use when asked to search for code patterns, find usages, refactor/rewrite code across many files, enforce coding conventions, or write ast-grep rules/YAML configs. Also triggered by: "find all X patterns", "rewrite X to Y", "lint for X", "write an ast-grep rule", "use sg to search".
---

# ast-grep Skill

ast-grep (`sg`) is a polyglot AST-based code search, lint, and rewrite tool. Unlike grep, it understands code structure.

## Mandatory Workflow: Script → Test → Iterate

**Always** follow this loop — never apply a rule to the codebase until it passes tests.

```
Write shell script → run sg test → analyze failures → fix rule → repeat → apply
```

### Step 1 — Write a shell script

Every ast-grep task starts as a self-contained shell script. The script creates a temp workspace, writes the rule and test cases, runs `sg test`, and (once tests pass) applies the rule.

Use this template:

```bash
#!/usr/bin/env bash
set -euo pipefail

WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT

# ── sgconfig ─────────────────────────────────────────────────────────
cat > "$WORK_DIR/sgconfig.yml" << 'SGCONFIG'
ruleDirs:
  - rules
testConfigs:
  - testDir: tests
SGCONFIG

mkdir -p "$WORK_DIR/rules" "$WORK_DIR/tests"

# ── Rule ─────────────────────────────────────────────────────────────
cat > "$WORK_DIR/rules/my-rule.yml" << 'RULE'
id: my-rule
language: JavaScript
severity: warning
message: "Prefer logger over console.log"
rule:
  pattern: console.log($$$ARGS)
fix: "logger.log($$$ARGS)"
RULE

# ── Test cases ───────────────────────────────────────────────────────
cat > "$WORK_DIR/tests/my-rule-test.yml" << 'TESTS'
id: my-rule
valid:
  - logger.log('hello')
  - console.warn('hello')
invalid:
  - console.log('hello')
  - console.log(a, b, c)
TESTS

# ── Run tests ────────────────────────────────────────────────────────
echo "--- Running tests ---"
sg test -c "$WORK_DIR/sgconfig.yml" --skip-snapshot-tests

# ── Apply (only reached if tests pass) ───────────────────────────────
echo "--- Tests passed. Applying rule ---"
sg scan --rule "$WORK_DIR/rules/my-rule.yml" --update-all .
```

### Step 2 — Run the script and read the output

Execute the script with `bash script.sh`. Test output tells you exactly what is wrong:

| Symbol | Meaning                                                 | Action           |
| ------ | ------------------------------------------------------- | ---------------- |
| `.`    | Correct match                                           | Good             |
| `N`    | **Noisy** — rule matched valid code (false positive)    | Tighten the rule |
| `M`    | **Missing** — rule missed invalid code (false negative) | Broaden the rule |

Example failure output:

```
FAIL my-rule  ..N..M

[Noisy] Expect my-rule to report no issue, but some issues found in:
  console.warn('hello')

[Missing] Expect rule my-rule to report issues, but none found in:
  console.log(a, b, c)
```

### Step 3 — Fix and iterate

Adjust the rule YAML inside the script. Common fixes:

- **Noisy** → add `not:`, tighten `pattern`, or add `constraints:`
- **Missing** → switch to `any:` for multiple patterns, use `$$$` for variadic args
- **Nothing matches at all** → check `language`, try `sg run --debug-query` to inspect the AST

**Re-run the script after every change. Do not stop iterating until the test output shows all `.` with no `N` or `M`.**

---

## Pattern Syntax Essentials

| Syntax     | Meaning                                             |
| ---------- | --------------------------------------------------- |
| `$NAME`    | Matches **one** AST node; captures as `$NAME`       |
| `$$$ARGS`  | Matches **zero or more** AST nodes (variadic)       |
| `$_`       | Matches one node, no capture (anonymous)            |
| `$_VAR`    | Non-capturing — same name can match different nodes |
| `$$VAR`    | Matches unnamed/anonymous nodes                     |
| `$A == $A` | Reuse same name = both must match same text         |

Pattern must be **valid parseable code**. For ambiguous snippets (e.g., bare `{x: y}`), use object-style:

```yaml
pattern:
  context: "const x = { $KEY: $VAL }"
  selector: pair
```

## Rule Object Quick Reference

**Atomic** (what to match):

- `pattern: expr` — match by code structure
- `kind: node_kind` — match by tree-sitter node type (use playground to find names)
- `regex: ^pattern$` — match node text with Rust regex

**Relational** (structural context):

- `inside: <rule>` — must be a descendant of matching node
- `has: <rule>` — must have a matching descendant
- `follows: <rule>` — must appear after matching sibling
- `precedes: <rule>` — must appear before matching sibling
- Add `stopBy: end` to search beyond immediate neighbors (default: `neighbor`)

**Composite** (logic):

- `all: [<rules>]` — all must match (AND)
- `any: [<rules>]` — any must match (OR)
- `not: <rule>` — must not match
- `matches: util-id` — reuse a named utility rule from `utils:`

**Extra fields in rule files:**

- `constraints:` — refine captured `$VAR` with additional rules
- `utils:` — local reusable named sub-rules
- `transform:` — manipulate captured variables before use in `fix`

## Finding Node Kinds

When you need a `kind:` value and don't know the exact name:

1. Run `sg run --debug-query --lang <lang> -p '<pattern>'` to dump the pattern AST
2. Check the [playground](https://ast-grep.github.io/playground.html) interactively

## Test Case Format

```yaml
id: rule-id # must match the rule's id field
valid:
  - "code that should NOT trigger the rule"
  - "another valid snippet"
invalid:
  - "code that SHOULD trigger the rule"
  - "another invalid snippet"
```

Run with `--skip-snapshot-tests` during iteration. Once the rule is stable, run without the flag to generate/validate snapshots of the exact error output.

## Reference Files

Load these when you need deeper detail:

- **`references/pattern-syntax.md`** — complete pattern syntax, meta-variable rules, strictness levels, edge cases
- **`references/rule-object.md`** — full rule object reference with all fields and examples
- **`references/yaml-config.md`** — complete YAML rule config (fix, transform, rewriters, severity, files/ignores, project config)
