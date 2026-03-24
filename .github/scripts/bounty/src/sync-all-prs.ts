#!/usr/bin/env tsx
// Syncs bounty labels across ALL open PRs that carry any "bounty" label.
//
// For each matching PR, the script fetches the linked issues from the PR body,
// then runs the full PR rules (label propagation + rewarded lifecycle).
//
// Without --execute: fetches all matching PRs, computes the combined patch,
// and prints a plan showing exactly what would change. No writes are made.
//
// With --execute: fetches, computes, and applies the patch for every PR.
//
// Usage:
//   tsx sync-all-prs.ts --repo <owner/repo> --token <token> [--execute]

import * as url from "url";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import { GitHubRestApi, type GitHubApi } from "./api.js";
import { computePrPatch, linkedIssueNumbers } from "./rules.js";
import { applyPatch, printPlan, resolveToken } from "./sync-issue.js";
import type { Patch } from "./types.js";

const BOUNTY_LABEL_PREFIX = "bounty";

export interface PlanAllPrsInput {
  api: GitHubApi;
}

/// Fetches all open PRs with any bounty label, resolves their linked issues,
/// and computes the combined patch. Makes no writes — safe to call at any time.
export async function planAllPrs({ api }: PlanAllPrsInput): Promise<Patch> {
  const prs = await api.listPrsWithLabelPrefix(BOUNTY_LABEL_PREFIX);

  const allOps = await Promise.all(
    prs.map(async (pr) => {
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

      return computePrPatch({ pr, currentLabels, linkedIssues }).ops;
    })
  );

  return { ops: allOps.flat() };
}

/// Fetches, computes, and applies the label patch for all bounty PRs.
/// Returns the patch that was applied.
export async function syncAllPrs({ api }: PlanAllPrsInput): Promise<Patch> {
  const patch = await planAllPrs({ api });
  await applyPatch(patch, api);
  return patch;
}

// ---------------------------------------------------------------------------
// CLI entrypoint
// ---------------------------------------------------------------------------

if (process.argv[1] === url.fileURLToPath(import.meta.url)) {
  const argv = await yargs(hideBin(process.argv))
    .option("repo", { type: "string", demandOption: true, description: "owner/repo" })
    .option("token", {
      type: "string",
      description: "GitHub token (falls back to GITHUB_TOKEN env var or `gh auth token`)",
    })
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
    const patch = await syncAllPrs({ api });
    if (patch.ops.length === 0) {
      console.log("All PRs already in sync — no changes needed.");
    }
  } else {
    const patch = await planAllPrs({ api });
    printPlan(patch, "All bounty PRs");
  }
}
