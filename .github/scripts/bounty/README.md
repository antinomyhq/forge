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

## Three-Step Pipeline

Each workflow job runs three steps in sequence, passing JSON through temp files:

```
parse  →  intent.json  →  plan  →  plan.json  →  execute
```

**Step 1 — parse** (`parse-*.ts`): Reads the GitHub Actions event payload from `GITHUB_EVENT_PATH` and emits a `ParsedIntent` JSON to stdout. Pure — makes no API calls.

**Step 2 — plan** (`plan.ts`): Reads `ParsedIntent`, fetches current labels only for targets not already known from the event payload, diffs desired vs actual state, and emits a `BatchPlan` JSON. Minimises API calls by using label data already present in the event.

**Step 3 — execute** (`execute.ts`): Reads `BatchPlan` and applies all mutations. Label additions per target are sent as a single batched `POST /labels` call. Each removal is a separate `DELETE` (GitHub has no bulk-remove endpoint). Comments are posted last.

This design means:
- The parse step is trivially unit-testable (pure function, no mocks needed).
- The plan step is testable with a minimal mock that only needs `getLabels`.
- The execute step is testable with a mock that tracks calls — no HTTP.
- API calls are minimised: additions are batched per target; label state already in the event payload is never re-fetched.

## Scripts

All scripts are invoked by `bounty.yml` via `npx tsx`. They read `GITHUB_EVENT_PATH` (set automatically by the runner) and accept CLI args parsed with `yargs`.

### `parse-propagate-label.ts`

Triggered by: `pull_request` — opened, edited, reopened.

1. Parses the PR body for closing keywords (`closes`, `fixes`, `resolves`, case-insensitive).
2. Emits a `ParsedIntent` with a `labelCopies` field — the plan step fetches each linked issue's labels and copies any `bounty: $N` ones onto the PR.
3. Includes a comment mutation per linked issue (the plan step drops it if the issue has no bounty labels).

```sh
npx tsx .github/scripts/bounty/parse-propagate-label.ts \
  --pr <number> < event.json > intent.json
```

### `parse-sync-claimed.ts`

Triggered by: `issues` — assigned, unassigned.

- **assigned**: emits `add: ["bounty: claimed"]` if the issue has a `bounty: $N` label.
- **unassigned**: emits `remove: ["bounty: claimed"]` only when no assignees remain.
- Issue labels are already in the event payload and supplied as `knownLabels` — no extra fetch.

```sh
npx tsx .github/scripts/bounty/parse-sync-claimed.ts \
  --issue <number> < event.json > intent.json
```

### `parse-sync-generic-bounty.ts`

Triggered by: `issues` — labeled, unlabeled.

Keeps the generic `bounty` label in sync with value labels. Inspects `event.label` (the label that just changed) and only acts when it matches `bounty: $`.

- **labeled**: emits `add: ["bounty"]`.
- **unlabeled**: emits `remove: ["bounty"]` only when no value labels remain (guards against mid-tier-swap removal when a maintainer swaps one value label for another).
- Issue labels from the event are supplied as `knownLabels` — no extra fetch.

```sh
npx tsx .github/scripts/bounty/parse-sync-generic-bounty.ts \
  --issue <number> < event.json > intent.json
```

### `parse-mark-rewarded.ts`

Triggered by: `pull_request_target` — closed (merged only).

1. Returns empty intent if the PR was not merged or has no `bounty: $N` label.
2. Emits `add: ["bounty: rewarded"]` for the PR (labels known from event, no fetch).
3. Parses the PR body for linked issues; emits `add: ["bounty: rewarded"], remove: ["bounty: claimed"]` for each (plan step fetches their labels).

Uses `pull_request_target` so the job has write access to issues and PRs from forks.

```sh
npx tsx .github/scripts/bounty/parse-mark-rewarded.ts \
  --pr <number> < event.json > intent.json
```

### `plan.ts`

```sh
INTENT_FILE=intent.json npx tsx .github/scripts/bounty/plan.ts \
  --repo <owner/repo> --token <github-token> > plan.json
```

Reads `INTENT_FILE` (falls back to stdin). Resolves `labelCopies` by fetching source issue labels. Fetches current labels for any target not already in `knownLabels`. Filters out no-op adds and removes.

### `execute.ts`

```sh
PLAN_FILE=plan.json npx tsx .github/scripts/bounty/execute.ts \
  --repo <owner/repo> --token <github-token>
```

Reads `PLAN_FILE` (falls back to stdin). For each mutation: one batched `POST` for all additions, one `DELETE` per removal, one `POST` per comment.

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
