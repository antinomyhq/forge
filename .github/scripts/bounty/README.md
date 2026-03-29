# Bounty Automation

Automates the full lifecycle of issue bounties — from label propagation when a PR is opened, through claiming when work begins, to rewarding when a PR is merged.

## Flow

```
Issue created
└── maintainer adds  bounty: $N  label
        │
        ├── parse-sync-generic-bounty.ts  →  adds generic  bounty  label
        │
        ▼
Issue assigned to contributor
└── parse-sync-claimed.ts  →  adds  bounty: claimed
        │
        ▼
Contributor opens PR with "Closes #N" / "Fixes #N" / "Resolves #N"
└── parse-propagate-label.ts  →  copies  bounty: $N  to PR
                              →  posts comment on issue: "PR #X opened by @author"
        │
        ▼
PR is merged
└── parse-mark-rewarded.ts  →  adds  bounty: rewarded  to PR
                            →  adds  bounty: rewarded  to linked issue(s)
                            →  removes  bounty: claimed  from linked issue(s)
        │
        ▼
Bounty lifecycle complete

Removal path:
maintainer removes last  bounty: $N  label
└── parse-sync-generic-bounty.ts  →  removes generic  bounty  label
```

## Labels

| Label                            | Applied to | Set by                                                      |
| -------------------------------- | ---------- | ----------------------------------------------------------- |
| `bounty: $100` … `bounty: $5500` | Issue      | Maintainer (manually)                                       |
| `bounty`                         | Issue      | `parse-sync-generic-bounty.ts` on value label add/remove    |
| `bounty: claimed`                | Issue      | `parse-sync-claimed.ts` on assignment                       |
| `bounty: rewarded`               | Issue + PR | `parse-mark-rewarded.ts` on merge                           |

Bounty values follow the Fibonacci sequence: **$100, $200, $300, $500, $800, $1300, $2100, $3400, $5500**.

## Pipeline

Each workflow job runs a single Node process (`run.ts`) that executes all three stages in memory — no temp files, no shell piping:

```
parse  →  ParsedIntent (in memory)  →  plan  →  BatchPlan (in memory)  →  execute
```

**Stage 1 — parse** (`parse-*.ts`): Reads the GitHub Actions event payload from `GITHUB_EVENT_PATH` and returns a `ParsedIntent` object. Pure — makes no API calls.

**Stage 2 — plan** (`plan.ts`): Receives the `ParsedIntent`, fetches current labels only for targets not already known from the event payload, diffs desired vs actual state, and returns a `BatchPlan`. Minimises API calls by reusing label data already present in the event.

**Stage 3 — execute** (`execute.ts`): Receives the `BatchPlan` and applies all mutations. Label additions per target are sent as a single batched `POST /labels` call. Each removal is a separate `DELETE` (GitHub has no bulk-remove endpoint). Comments are posted last.

This design means:
- The parse stage is trivially unit-testable (pure function, no mocks needed).
- The plan stage is testable with a minimal mock that only needs `getLabels`.
- The execute stage is testable with a mock that tracks calls — no HTTP.
- API calls are minimised: additions are batched per target; label state already in the event payload is never re-fetched.

## Scripts

The workflow invokes a single orchestrator script (`run.ts`) per job. The parse, plan, and execute modules are called directly in the same Node process, passing objects in memory.

### `run.ts`

The entrypoint used by `bounty.yml`. Accepts a `--script` flag to select which parse module to run, then calls `plan` and `execute` in sequence — all in-process.

```sh
npx tsx .github/scripts/bounty/run.ts \
  --script <parse-script> \
  --repo <owner/repo> \
  --token <github-token> \
  [--pr <number> | --issue <number>]
```

### `parse-propagate-label.ts`

Triggered by: `pull_request` — opened, edited, reopened.

1. Parses the PR body for closing keywords (`closes`, `fixes`, `resolves`, case-insensitive).
2. Returns a `ParsedIntent` with a `labelCopies` field — the plan stage fetches each linked issue's labels and copies any `bounty: $N` ones onto the PR.
3. Includes a comment mutation per linked issue (the plan stage drops it if the issue has no bounty labels).

### `parse-sync-claimed.ts`

Triggered by: `issues` — assigned, unassigned.

- **assigned**: returns `add: ["bounty: claimed"]` if the issue has a `bounty: $N` label.
- **unassigned**: returns `remove: ["bounty: claimed"]` only when no assignees remain.
- Issue labels are already in the event payload and supplied as `knownLabels` — no extra fetch.

### `parse-sync-generic-bounty.ts`

Triggered by: `issues` — labeled, unlabeled.

Keeps the generic `bounty` label in sync with value labels. Inspects `event.label` (the label that just changed) and only acts when it matches `bounty: $`.

- **labeled**: returns `add: ["bounty"]`.
- **unlabeled**: returns `remove: ["bounty"]` only when no value labels remain (guards against mid-tier-swap removal when a maintainer swaps one value label for another).
- Issue labels from the event are supplied as `knownLabels` — no extra fetch.

### `parse-mark-rewarded.ts`

Triggered by: `pull_request_target` — closed (merged only).

1. Returns empty intent if the PR was not merged or has no `bounty: $N` label.
2. Returns `add: ["bounty: rewarded"]` for the PR (labels known from event, no fetch).
3. Parses the PR body for linked issues; returns `add: ["bounty: rewarded"], remove: ["bounty: claimed"]` for each (plan stage fetches their labels).

Uses `pull_request_target` so the job has write access to issues and PRs from forks.

### `plan.ts`

Receives a `ParsedIntent` from the parse stage. Resolves `labelCopies` by fetching source issue labels. Fetches current labels for any target not already in `knownLabels`. Filters out no-op adds and removes. Returns a `BatchPlan`.

### `execute.ts`

Receives a `BatchPlan` from the plan stage. For each mutation: one batched `POST` for all additions, one `DELETE` per removal, one `POST` per comment.

## Shared Module

`github-api.ts` defines:
- Event payload types (`PullRequestEvent`, `IssuesEvent`)
- Pipeline types (`ParsedIntent`, `BatchPlan`, `TargetMutation`)
- The `GitHubApi` interface (injectable for testing)
- `GitHubRestApi` — the production implementation using `node:https`

All scripts import types from `github-api.ts` and accept a `GitHubApi` instance in their `run()` / `plan()` / `execute()` function signature, making every step independently mockable.

## Tests

Unit tests live alongside each script (`*.test.ts`) and use Node's built-in `node:test` runner.

```sh
npm run test:bounty
```

- Parse tests: pure — no mock needed, just call `parse()` with a synthetic event.
- Plan and execute tests: use a mock `GitHubApi` that tracks calls and returns preset label lists.
- The CLI entrypoint in each script (yargs parsing + `GITHUB_EVENT_PATH` read) is guarded behind an `import.meta.url` check so it does not execute on import.

## Workflow Source

`bounty.yml` is auto-generated from Rust source in `crates/forge_ci`. Do not edit it by hand — modify `crates/forge_ci/src/workflows/bounty.rs` and regenerate with:

```sh
cargo test -p forge_ci
```
