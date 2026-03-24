#!/usr/bin/env tsx
// Syncs bounty labels across ALL open issues that carry any "bounty" label.
//
// Without --execute: fetches all matching issues, computes the combined patch,
// and prints a plan showing exactly what would change. No writes are made.
//
// With --execute: fetches, computes, and applies the patch for every issue.
//
// Usage:
//   tsx sync-all-issues.ts --repo <owner/repo> --token <token> [--execute]

import * as url from "url";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import { GitHubRestApi, type GitHubApi } from "./api.js";
import { computeIssuePatch } from "./rules.js";
import { applyPatch, printPlan, resolveToken } from "./sync-issue.js";
import type { Patch } from "./types.js";

const BOUNTY_LABEL_PREFIX = "bounty";

export interface PlanAllIssuesInput {
  api: GitHubApi;
}

/// Fetches all open issues with any bounty label and computes the combined patch.
/// Makes no writes — safe to call at any time.
export async function planAllIssues({ api }: PlanAllIssuesInput): Promise<Patch> {
  const issues = await api.listIssuesWithLabelPrefix(BOUNTY_LABEL_PREFIX);
  const ops = issues.flatMap((issue) => {
    const currentLabels = new Set(issue.labels.map((l) => l.name));
    return computeIssuePatch({ issue, currentLabels }).ops;
  });
  return { ops };
}

/// Fetches, computes, and applies the label patch for all bounty issues.
/// Returns the patch that was applied.
export async function syncAllIssues({ api }: PlanAllIssuesInput): Promise<Patch> {
  const patch = await planAllIssues({ api });
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
    const patch = await syncAllIssues({ api });
    if (patch.ops.length === 0) {
      console.log("All issues already in sync — no changes needed.");
    }
  } else {
    const patch = await planAllIssues({ api });
    printPlan(patch, "All bounty issues");
  }
}
