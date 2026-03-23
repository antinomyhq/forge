# Bounty Automation

Automates the full lifecycle of issue bounties — from label propagation when a PR is opened, through claiming when work begins, to rewarding when a PR is merged.

## Flow

```
Issue created
└── maintainer adds  bounty: $N  label
        │
        ├── sync-generic-bounty.ts  →  adds generic  bounty  label
        │
        ▼
Issue assigned to contributor
└── sync-claimed.ts  →  adds  bounty: claimed
        │
        ▼
Contributor opens PR with "Closes #N" / "Fixes #N" / "Resolves #N"
└── propagate-label.ts  →  copies  bounty: $N  to PR
                        →  posts comment on issue: "PR #X opened by @author"
        │
        ▼
PR is merged
└── mark-rewarded.ts  →  adds  bounty: rewarded  to PR
                     →  adds  bounty: rewarded  to linked issue(s)
                     →  removes  bounty: claimed  from linked issue(s)
        │
        ▼
Bounty lifecycle complete

Removal path:
maintainer removes last  bounty: $N  label
└── sync-generic-bounty.ts  →  removes generic  bounty  label
```

## Labels

| Label                            | Applied to | Set by                                             |
| -------------------------------- | ---------- | -------------------------------------------------- |
| `bounty: $100` … `bounty: $5500` | Issue      | Maintainer (manually)                              |
| `bounty`                         | Issue      | `sync-generic-bounty.ts` on value label add/remove |
| `bounty: claimed`                | Issue      | `sync-claimed.ts` on assignment                    |
| `bounty: rewarded`               | Issue + PR | `mark-rewarded.ts` on merge                        |

Bounty values follow the Fibonacci sequence: **$100, $200, $300, $500, $800, $1300, $2100, $3400, $5500**.

## Scripts

All four scripts are invoked by the `bounty.yml` GitHub Actions workflow via `npx tsx`. They read the GitHub Actions event payload from the `GITHUB_EVENT_PATH` environment variable (set automatically by the runner) and call the GitHub REST API using a provided token.

### `propagate-label.ts`

Triggered by: `pull_request` — opened, edited, reopened.

1. Parses the PR body for closing keywords (`closes`, `fixes`, `resolves`, case-insensitive).
2. For each linked issue number, fetches its labels.
3. Copies any `bounty: $N` label to the PR (skipped if the PR already has it).
4. Posts a comment on the issue: `PR <url> has been opened for this bounty by @<author>.`

```sh
npx tsx .github/scripts/bounty/propagate-label.ts \
  --pr <number> \
  --repo <owner/repo> \
  --token <github-token>
```

### `sync-claimed.ts`

Triggered by: `issues` — assigned, unassigned.

- **assigned**: adds `bounty: claimed` to the issue, but only if the issue already has a `bounty: $N` label and does not already have `bounty: claimed`.
- **unassigned**: removes `bounty: claimed` only when no assignees remain on the issue.

```sh
npx tsx .github/scripts/bounty/sync-claimed.ts \
  --issue <number> \
  --repo <owner/repo> \
  --token <github-token>
```

### `sync-generic-bounty.ts`

Triggered by: `issues` — labeled, unlabeled.

Keeps the generic `bounty` label in sync with the presence of any `bounty: $N` value label. Inspects `event.label` (the label that changed) and only acts when it matches the value label pattern.

- **labeled**: adds `bounty` to the issue (skipped if already present).
- **unlabeled**: removes `bounty` only when no value labels remain on the issue (guards against removing it mid-tier-change when a maintainer swaps one value label for another).

```sh
npx tsx .github/scripts/bounty/sync-generic-bounty.ts \
  --issue <number> \
  --repo <owner/repo> \
  --token <github-token>
```

### `mark-rewarded.ts`

Triggered by: `pull_request_target` — closed (merged only).

1. Checks the PR was actually merged (not just closed).
2. Checks the PR has a `bounty: $N` label (skips otherwise).
3. Adds `bounty: rewarded` to the PR.
4. Parses the PR body for linked issues (same closing keyword pattern).
5. For each linked issue: adds `bounty: rewarded` and removes `bounty: claimed`.

Uses `pull_request_target` (instead of `pull_request`) so the job has write access to issues and PRs originating from forks.

```sh
npx tsx .github/scripts/bounty/mark-rewarded.ts \
  --pr <number> \
  --repo <owner/repo> \
  --token <github-token>
```

## Shared module

`github-api.ts` defines the `GitHubApi` interface and the production `GitHubRestApi` implementation. All four scripts depend on this interface rather than a concrete class, which makes the `run()` function in each script fully testable with a mock.

## Tests

Unit tests live alongside each script (`*.test.ts`) and use Node's built-in `node:test` runner with a mock `GitHubApi`. No external test framework is needed.

```sh
npm run test:bounty
```

The CLI entrypoint in each script (yargs arg parsing + `GITHUB_EVENT_PATH` read) is guarded behind an `import.meta.url` check so it does not execute when the file is imported by a test.

## Workflow source

The `bounty.yml` workflow is auto-generated from Rust source in `crates/forge_ci`. Do not edit it by hand — modify `crates/forge_ci/src/workflows/bounty.rs` and regenerate with:

```sh
cargo test -p forge_ci
```
