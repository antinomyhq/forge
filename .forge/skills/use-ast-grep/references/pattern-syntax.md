# Pattern Syntax Reference

## Meta Variables

Meta variables start with `$` followed by uppercase letters, `_`, or digits `1-9`.

| Syntax    | Description                                                           |
| --------- | --------------------------------------------------------------------- |
| `$VAR`    | Matches exactly **one** named AST node; captured as `$VAR`            |
| `$$$ARGS` | Matches **zero or more** AST nodes (use for args, params, statements) |
| `$_`      | Matches one node without capturing (anonymous)                        |
| `$_VAR`   | Non-capturing named variable — same name can match different nodes    |
| `$$VAR`   | Matches unnamed/anonymous AST nodes                                   |

### Valid meta variable names

- `$META`, `$META_VAR`, `$META_VAR1`, `$_`, `$_123` ✅
- `$invalid`, `$Svalue`, `$123`, `$KEBAB-CASE`, `$` ❌

## Single vs Multi Meta Variables

`$X` matches exactly one AST node:

```
console.log($ARG)  // matches console.log('hello') but NOT console.log(a, b)
```

`$$$ARGS` matches zero or more nodes:

```
console.log($$$ARGS)  // matches console.log(), console.log(a), console.log(a, b, c)
```

`function $FUNC($$$PARAMS) { $$$BODY }` — use `$$$` for params and body.

## Capture Groups (Backreferences)

Reusing the same meta variable name enforces the two positions must match the same text:

```
$A == $A    // matches: a == a, (x+1) == (x+1)
            // NOT:     a == b
```

## Non-Capturing Variables

Prefix name with `_` to suppress capture (useful for performance):

```
$_FUNC($_FUNC)   // matches any single-arg call, both positions can differ
```

## Nested Matching

Patterns match anywhere in the AST tree:

```
a + 1   // matches: const b = a + 1, funcCall(a + 1), {target: a + 1}
```

## Ambiguous Patterns

Some code is ambiguous without context. Use object-style pattern with `context` and `selector`:

```yaml
# Object literal `{key: val}` is ambiguous - is it a block or object?
pattern:
  context: "const x = { $KEY: $VAL }"
  selector: pair

# Class field definition
pattern:
  selector: field_definition
  context: "class { $F }"
```

The `selector` is a tree-sitter node kind name. Use the playground to find it.

## Pattern Strictness

Control how strictly the pattern matches with `strictness`:

| Level       | Description                                               |
| ----------- | --------------------------------------------------------- |
| `cst`       | Match concrete syntax tree exactly (most strict)          |
| `smart`     | Default: ignore some trivial nodes (comments, whitespace) |
| `ast`       | Ignore all unnamed nodes                                  |
| `relaxed`   | Match named nodes only, ignore extra unnamed nodes        |
| `signature` | Match function signature only                             |

```yaml
pattern:
  context: foo($BAR)
  strictness: relaxed
```

## Pattern Gotchas

- Pattern must be **valid parseable code** — `ast-grep` uses tree-sitter to parse it
- Comments inside patterns are typically ignored
- `console.log(123) in a string or comment` is **not** matched
- If a pattern fails to parse, try the playground or add more context with object-style pattern
- Single `$VAR` won't match zero nodes — use `$$$` for optional/variadic

## Language-Specific Notes

- Pattern is interpreted in the rule's `language` context
- String literal syntax varies: `'hello'` is valid in JS/TS but not in Rust/C (use `"hello"`)
- Use `--lang` flag when running CLI to ensure correct parsing
