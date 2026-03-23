// Executes a BatchPlan against the GitHub API.
// Additions per target are sent as a single batched call.
// Each removal is a separate call (GitHub has no bulk-remove endpoint).
//
// Usage:
//   tsx execute.ts --repo <owner/repo> --token <token> < plan.json

import * as fs from "fs";
import * as url from "url";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import {
  GitHubRestApi,
  type GitHubApi,
  type BatchPlan,
} from "./github-api.js";

export interface ExecuteInput {
  plan: BatchPlan;
  api: GitHubApi;
}

/// Applies all mutations in a BatchPlan.
/// Each target gets at most one addLabels call (batch) and one removeLabel
/// call per label to remove.
export async function execute({ plan, api }: ExecuteInput): Promise<void> {
  for (const { target, add, remove, comment } of plan.mutations) {
    if (add.length > 0) {
      await api.addLabels(target, add);
      console.log(`#${target}: added [${add.join(", ")}]`);
    }

    for (const label of remove) {
      await api.removeLabel(target, label);
      console.log(`#${target}: removed "${label}"`);
    }

    if (comment) {
      await api.addComment(target, comment);
      console.log(`#${target}: posted comment`);
    }
  }
}

// ---------------------------------------------------------------------------
// CLI entrypoint — only runs when executed directly (not imported by tests)
// ---------------------------------------------------------------------------

if (process.argv[1] === url.fileURLToPath(import.meta.url)) {
  const argv = await yargs(hideBin(process.argv))
    .option("repo", { type: "string", demandOption: true, description: "owner/repo" })
    .option("token", { type: "string", demandOption: true, description: "GitHub token" })
    .strict()
    .parseAsync();

  const planJson = fs.readFileSync(process.env["PLAN_FILE"] ?? "/dev/stdin", "utf8");
  const batchPlan = JSON.parse(planJson) as BatchPlan;
  const [owner, repo] = argv.repo.split("/") as [string, string];

  await execute({ plan: batchPlan, api: new GitHubRestApi(owner, repo, argv.token) });
}
