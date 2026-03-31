# Rule Object Reference

A rule object can combine atomic, relational, and composite rules. All keys are optional, but at least one must be present. A node matches only if **all** specified keys match (implicit `all`).

## Table of Contents

1. [Atomic Rules](#atomic-rules)
2. [Relational Rules](#relational-rules)
3. [Composite Rules](#composite-rules)
4. [Rule Ordering Caveat](#rule-ordering-caveat)

---

## Atomic Rules

### `pattern`

Match by code structure. Accepts a string or an object.

```yaml
# String form
rule:
  pattern: console.log($ARG)

# Object form - for ambiguous patterns
rule:
  pattern:
    context: "class { $F }"
    selector: field_definition
    strictness: relaxed  # optional: cst | smart | ast | relaxed | signature
```

### `kind`

Match by tree-sitter node kind name.

```yaml
rule:
  kind: call_expression

# ESQuery-style child selector (0.39+)
rule:
  kind: call_expression > identifier
```

Find kind names in the [playground](https://ast-grep.github.io/playground.html) or by running:

```bash
sg run --debug-query --lang <lang> -p '<pattern>'
```

### `regex`

Match by node text using a Rust regex. The regex must match the **entire** node text.

```yaml
rule:
  regex: ^[A-Z][a-zA-Z]+$ # PascalCase identifier


# Rust regex syntax - no lookahead/lookbehind, no backreferences
# Flags: (?i) case-insensitive, (?m) multiline
```

### `nthChild`

Match nodes by their position among named siblings (1-based index).

```yaml
# Exact position
nthChild: 1   # first named sibling

# An+B formula (like CSS :nth-child)
nthChild: 2n+1  # odd positions

# Object form with options
nthChild:
  position: 2n+1
  reverse: true        # count from end
  ofRule:              # filter sibling list by rule first
    kind: argument
```

### `range`

Match a node by its exact character span (0-based, start inclusive, end exclusive).

```yaml
range:
  start: { line: 0, column: 0 }
  end: { line: 0, column: 13 }
```

---

## Relational Rules

Relational rules filter nodes based on their structural relationships. Each accepts a sub-rule (any rule object) plus optional `stopBy` and `field`.

### `stopBy` (shared option)

Controls how far to search in the given direction:

- `"neighbor"` (default) — only immediate parent/child/sibling
- `"end"` — search all the way to root (for `inside`) or leaves (for `has`)
- `<rule object>` — stop when a node matching this rule is encountered (inclusive)

### `field` (inside/has only)

Restrict to a specific semantic field of the parent node (e.g., `body`, `condition`, `name`).

### `inside`

Target node must appear inside (be a descendant of) a node matching the sub-rule.

```yaml
rule:
  pattern: $X.unwrap()
  inside:
    kind: function_item
    stopBy: end
    # field: body   # optional, constrain to specific field
```

### `has`

Target node must have a descendant matching the sub-rule.

```yaml
rule:
  pattern: Promise.all($A)
  has:
    pattern: await $_
    stopBy: end

# With field
rule:
  kind: function_declaration
  has:
    kind: statement_block
    field: body
```

### `precedes`

Target node must appear before a sibling node matching the sub-rule.

```yaml
rule:
  precedes:
    kind: function_declaration
    stopBy: end
```

### `follows`

Target node must appear after a sibling node matching the sub-rule.

```yaml
rule:
  follows:
    pattern: let x = 10;
    stopBy: neighbor
```

---

## Composite Rules

### `all`

Node must satisfy **all** sub-rules (AND logic). Meta-variables from all sub-rules are merged.

```yaml
rule:
  all:
    - kind: call_expression
    - pattern: console.log($ARG)
    - not:
        inside:
          kind: catch_clause
          stopBy: end
```

### `any`

Node must satisfy **at least one** sub-rule (OR logic). Meta-variables come from the matched sub-rule only.

```yaml
rule:
  any:
    - pattern: console.log($$$)
    - pattern: console.warn($$$)
    - pattern: console.error($$$)
```

### `not`

Node must **not** satisfy the sub-rule.

```yaml
rule:
  pattern: $FUNC($$$ARGS)
  not:
    inside:
      kind: test_function
      stopBy: end
```

### `matches`

Reuse a named utility rule (defined in `utils:` or a global util file).

```yaml
utils:
  is-function:
    any:
      - kind: function_declaration
      - kind: arrow_function
      - kind: function

rule:
  matches: is-function
  has:
    pattern: console.log($$$)
    stopBy: end
```

---

## Rule Ordering Caveat

The rule object is **unordered** — ast-grep may apply sub-rules in any order. This matters for `has`/`inside` combined with other rules.

If a rule doesn't work as expected, use explicit `all:` to control order:

```yaml
# Instead of:
rule:
  pattern: foo($A)
  inside:
    kind: class_body

# Try:
rule:
  all:
    - pattern: foo($A)
    - inside:
        kind: class_body
```
