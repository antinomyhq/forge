---
name: use-ast-grep
description: Use ast-grep (sg) for structural code search, linting, and rewriting based on AST patterns rather than text. Use when asked to search for code patterns, find usages, refactor/rewrite code across many files, enforce coding conventions, or write ast-grep rules/YAML configs. Also triggered by: "find all X patterns", "rewrite X to Y", "lint for X", "write an ast-grep rule", "use sg to search".
---

# ast-grep Skill

ast-grep (`sg`) is a polyglot AST-based code search, lint, and rewrite tool. Unlike grep, it understands code structure.

## Decision: CLI one-liner vs YAML rule

- **One-liner** (`sg run`): quick search or simple rewrite, no persistence needed
- **YAML rule** (`sg scan --rule`): complex matching, linting with messages, project-wide enforcement, or automated fix

## Quick CLI Usage

```bash
# Search
sg --pattern 'console.log($ARGS)' --lang js

# Search + rewrite (preview only)
sg --pattern 'var $X = $Y' --rewrite 'let $X = $Y' --lang js

# Apply rewrites interactively
sg --pattern 'var $X = $Y' --rewrite 'let $X = $Y' --lang js -i

# Apply all rewrites without confirmation
sg --pattern 'var $X = $Y' --rewrite 'let $X = $Y' --lang js -U

# Search specific files/dirs
sg --pattern 'foo($ARG)' --lang rust src/
```

## Pattern Syntax Essentials

| Syntax     | Meaning                                       |
| ---------- | --------------------------------------------- |
| `$NAME`    | Matches **one** AST node; captures as `$NAME` |
| `$$$ARGS`  | Matches **zero or more** AST nodes (variadic) |
| `$_`       | Matches one node, no capture (anonymous)      |
| `$$VAR`    | Matches unnamed/anonymous nodes               |
| `$A == $A` | Reuse same name = both must match same text   |

**Pattern must be valid parseable code.** If the pattern is ambiguous (e.g., a bare object literal `{x: y}`), use object-style pattern with `context`/`selector`.

## YAML Rule Workflow

### 1. Write the rule file

```yaml
id: no-console-log
language: JavaScript
severity: warning
message: "Avoid console.log in production"
rule:
  pattern: console.log($$$ARGS)
fix: "logger.log($$$ARGS)"
```

### 2. Run against codebase

```bash
# Single rule file
sg scan --rule no-console-log.yml src/

# Inline (no file needed)
sg scan --inline-rules '
id: my-rule
language: Rust
rule:
  pattern: println!($$$)
' src/

# Apply fixes automatically
sg scan --rule no-console-log.yml -U src/
```

### 3. Compose rules for complex patterns

```yaml
id: await-in-promise-all
language: TypeScript
rule:
  pattern: Promise.all($A)
  has:
    pattern: await $_
    stopBy: end
```

## Rule Object Quick Reference

**Atomic** (what to match):

- `pattern: expr` — match by code structure
- `kind: node_kind` — match by tree-sitter node type
- `regex: ^pattern$` — match node text with Rust regex

**Relational** (context filtering):

- `inside: <rule>` — must be a descendant of matching node
- `has: <rule>` — must have a matching descendant
- `follows: <rule>` — must appear after matching sibling
- `precedes: <rule>` — must appear before matching sibling
- Add `stopBy: end` to search beyond immediate neighbors

**Composite** (logic):

- `all: [<rules>]` — all must match
- `any: [<rules>]` — any must match
- `not: <rule>` — must not match
- `matches: util-id` — reuse a named utility rule from `utils:`

**Modifiers:**

- `constraints:` — additional rules on captured meta-variables
- `utils:` — define reusable named sub-rules locally
- `transform:` — manipulate captured variables for the `fix`

## Finding Node Kinds

When you need a `kind:` value but don't know the exact name:

1. Use the [ast-grep playground](https://ast-grep.github.io/playground.html) to inspect the AST
2. Run `sg run --debug-query --lang <lang> -p '<pattern>'` to dump the pattern tree

## Common Patterns

```yaml
# Find function calls with specific arg count
rule:
  kind: call_expression
  has:
    kind: arguments
    has:
      nthChild: 3  # exactly 3 args (1-based, named nodes only)

# Find inside specific context
rule:
  pattern: $X.unwrap()
  inside:
    kind: function_item
    stopBy: end

# Match with constrained meta-variable
rule:
  pattern: foo($ARG)
constraints:
  ARG:
    kind: string

# Multiple alternatives
rule:
  any:
    - pattern: console.log($$$)
    - pattern: console.warn($$$)
    - pattern: console.error($$$)
fix: ""  # delete all matches
```

## Reference Files

Load these when you need detailed information:

- **`references/pattern-syntax.md`** — complete pattern syntax, meta-variable rules, edge cases
- **`references/rule-object.md`** — full rule object reference with all fields
- **`references/yaml-config.md`** — complete YAML rule config (id, severity, fix, transform, files/ignores, etc.)
