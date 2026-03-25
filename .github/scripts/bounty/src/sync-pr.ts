#!/usr/bin/env tsx
// Syncs bounty labels on a PR (and its linked issues).
//
// Without --execute: fetches current state, computes the patch, and prints
// a plan showing exactly what would change. No writes are made.
//
// With --execute: fetches, computes, and applies the patch.
//
// Usage:
//   tsx sync-pr.ts --pr <number> --repo <owner/repo> --token <token> [--execute]

import * as url from "url";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import { GitHubRestApi, type GitHubApi } from "./api.js";
import { computePrPatch, linkedIssueNumbers } from "./rules.js";
import { applyPatch, printPlan, resolveToken } from "./sync-issue.js";
import type { Patch } from "./types.js";

export interface PlanPrInput {
  prNumber: number;
  api: GitHubApi;
}

/// Fetches the current PR + linked issue state and computes the minimal label patch.
/// Makes no writes — safe to call at any time.
export async function planPr({ prNumber, api }: PlanPrInput): Promise<Patch> {
  const pr = await api.getPullRequest(prNumber);
  const currentLabels = new Set(pr.labels.map((l) => l.name));
  const issueNumbers = linkedIssueNumbers(pr.body);

  const linkedIssues = await Promise.all(
    issueNumbers.map((n) =>
      api.getIssue(n).catch((err) => {
        console.warn(`Could not fetch linked issue #${n}: ${String(err)}, skipping.`);
        return null;
      })
    )
  ).then((results) => results.filter((i): i is NonNullable<typeof i> => i !== null));

  return computePrPatch({ pr, currentLabels, linkedIssues });
}

/// Fetches, computes, and applies the label patch for a PR and its linked issues.
/// Returns the patch that was applied (useful for tests and logging).
export async function syncPr({ prNumber, api }: PlanPrInput): Promise<Patch> {
  const patch = await planPr({ prNumber, api });
  await applyPatch(patch, api);
  return patch;
}

// ---------------------------------------------------------------------------
// CLI entrypoint
// ---------------------------------------------------------------------------

if (process.argv[1] === url.fileURLToPath(import.meta.url)) {
  const argv = await yargs(hideBin(process.argv))
    .option("pr", { type: "number", demandOption: true, description: "PR number" })
    .option("repo", { type: "string", demandOption: true, description: "owner/repo" })
    .option("token", { type: "string", description: "GitHub token (falls back to GITHUB_TOKEN env var or `gh auth token`)" })
    .option("execute", {
      type: "boolean",
      default: false,
      description: "Apply the patch. Without this flag only the plan is printed.",
    })
    .strict()
    .parseAsync();

  const [owner, repo] = argv.repo.split("/") as [string, string];
  const token = resolveToken(argv.token);
  const api = new GitHubRestApi(owner, repo, token);

  if (argv.execute) {
    const patch = await syncPr({ prNumber: argv.pr, api });
    if (patch.ops.length === 0) {
      console.log(`PR #${argv.pr}: already in sync, no changes needed.`);
    }
  } else {
    const patch = await planPr({ prNumber: argv.pr, api });
    printPlan(patch, `PR #${argv.pr}`);
  }
}
