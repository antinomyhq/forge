# YAML Rule Config Reference

A YAML rule file can contain multiple rules separated by `---`. Each rule is a YAML object.

## Table of Contents

1. [Required Fields](#required-fields)
2. [Finding Fields](#finding-fields)
3. [Patching Fields](#patching-fields)
4. [Linting Fields](#linting-fields)
5. [Globbing Fields](#globbing-fields)
6. [Project Config (sgconfig.yml)](#project-config)

---

## Required Fields

### `id`

Unique identifier for the rule. Used in CLI filtering and error reports.

```yaml
id: no-console-log
```

### `language`

Language to parse and scan. Determines which file extensions are matched.

Valid values: `Bash`, `C`, `Cpp`, `CSharp`, `Css`, `Elixir`, `Go`, `Haskell`, `Hcl`, `Html`, `Java`, `JavaScript`, `Json`, `Kotlin`, `Lua`, `Nix`, `Php`, `Python`, `Ruby`, `Rust`, `Scala`, `Solidity`, `Swift`, `Tsx`, `TypeScript`, `Yaml`

```yaml
language: TypeScript
```

---

## Finding Fields

### `rule`

The core match rule. Accepts a [rule object](rule-object.md). Required.

```yaml
rule:
  pattern: console.log($$$ARGS)
```

### `constraints`

Extra rules applied to captured meta-variables (single `$VAR` only, not `$$$VAR`).

```yaml
rule:
  pattern: foo($ARG)
constraints:
  ARG:
    kind: string # or pattern, regex, any, all, etc.
```

### `utils`

Local utility rules reusable within this file via `matches:`.

```yaml
utils:
  is-async-func:
    any:
      - kind: async_function
      - kind: arrow_function
        has:
          pattern: async $_
rule:
  matches: is-async-func
```

---

## Patching Fields

### `fix`

Auto-fix replacement. Can reference meta-variables from the rule.

```yaml
fix: "logger.log($$$ARGS)"

# Delete the match
fix: ""

# Multi-line fix
fix: |
  logger.warn($ARG)
```

#### FixConfig object (advanced)

```yaml
fix:
  template: "logger.$METHOD($$$ARGS)"
  expandEnd:
    kind: ";" # expand match to include trailing semicolon
```

### `transform`

Manipulate meta-variables to create new variables for use in `fix`.

```yaml
transform:
  UPPER_NAME:
    convert:
      source: $NAME
      toCase: upperCase    # upperCase | lowerCase | camelCase | snakeCase | etc.

  REPLACED:
    replace:
      source: $ARGS
      replace: "old"
      by: "new"

# String shorthand (0.38.3+)
transform:
  UPPER_NAME: convert($NAME, toCase=upperCase)
```

### `rewriters`

Define inline rewriter rules for use with `rewrite` transformation.

```yaml
rewriters:
  - id: stringify
    rule:
      pattern: "'' + $A"
    fix: "String($A)"

transform:
  NEW_BODY:
    rewrite:
      source: $BODY
      rewriters: [stringify]
```

---

## Linting Fields

### `severity`

Report level for matched nodes.

- `hint` | `info` | `warning` | `error` — report at this level
- `off` — disable the rule

```yaml
severity: warning
```

Override at scan time: `sg scan --error rule-id` or `--warning rule-id`

### `message`

Single-line description of why the rule fired. Can reference meta-variables.

```yaml
message: "Avoid $FUNC in production code. Use logger instead."
```

### `note`

Additional explanation or guidance. Supports markdown. Cannot reference meta-variables.

```yaml
note: "See https://example.com/logging-guide for alternatives."
```

### `labels`

Customize highlighting in editor extensions for specific meta-variables.

```yaml
labels:
  ARG:
    style: primary # primary | secondary
    message: "This argument is problematic"
  FUNC:
    style: secondary
```

### `url`

Documentation link displayed in editor integrations.

```yaml
url: "https://example.com/rules/no-console-log"
```

### `metadata`

Arbitrary key-value data for external tooling (output with `--json --include-metadata`).

```yaml
metadata:
  cve: "CVE-2021-12345"
  category: security
```

---

## Globbing Fields

### `files`

Glob patterns to restrict which files this rule applies to.

```yaml
files:
  - "src/**/*.ts"
  - "lib/**/*.js"

# Case-insensitive object form
files:
  - glob: "**/*.TS"
    caseInsensitive: true
```

Paths are relative to the `sgconfig.yml` directory. Do **not** prefix with `./`.

### `ignores`

Glob patterns to exclude files from this rule.

```yaml
ignores:
  - "**/*.test.ts"
  - "node_modules/**"
```

Processing order: `ignores` checked first → if no match, check `files` → if no `files`, include all.

---

## Project Config

`sgconfig.yml` at the project root configures the project-wide scan:

```yaml
# sgconfig.yml
ruleDirs:
  - rules/ # directories containing rule YAML files

testConfigs:
  - testDir: tests/ # test case directories

utilsDirs:
  - utils/ # global utility rules shared across rule files
```

### Running a project scan

```bash
# Scan with project config
sg scan

# Scan specific path
sg scan src/

# Use a specific config file
sg scan -c path/to/sgconfig.yml
```

### Initializing a new project

```bash
sg new project        # scaffold sgconfig.yml + rules/ + tests/
sg new rule my-rule   # create a new rule file
sg new test my-rule   # create a test case for a rule
```

---

## Full Rule Example

```yaml
id: prefer-logger-over-console
language: TypeScript
severity: warning
message: "Use logger.$METHOD instead of console.$METHOD"
note: "Direct console usage leaks to production logs. Import logger from '@/utils/logger'."
rule:
  any:
    - pattern: console.log($$$ARGS)
    - pattern: console.warn($$$ARGS)
    - pattern: console.error($$$ARGS)
  not:
    inside:
      kind: catch_clause
      stopBy: end
transform:
  METHOD:
    substring:
      source: $0 # full match text
      startChar: 8 # skip "console."
fix: "logger.$METHOD($$$ARGS)"
files:
  - "src/**/*.ts"
ignores:
  - "src/**/*.test.ts"
  - "src/**/*.spec.ts"
url: "https://example.com/eslint-rules#prefer-logger"
```
