#!/usr/bin/env tsx
// Syncs all bounty labels on a single issue.
//
// Without --execute: fetches current state, computes the patch, and prints
// a plan showing exactly what would change. No writes are made.
//
// With --execute: fetches, computes, and applies the patch.
//
// Usage:
//   tsx sync-issue.ts --issue <number> --repo <owner/repo> --token <token> [--execute]

import * as url from "url";
import { execSync } from "child_process";
import chalk from "chalk";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import { GitHubRestApi, type GitHubApi } from "./api.js";
import { computeIssuePatch } from "./rules.js";
import type { Patch } from "./types.js";

/// Resolves a GitHub token from the provided value, the GITHUB_TOKEN env var,
/// or by invoking `gh auth token` as a fallback. Exits with an error if none
/// can be found.
export function resolveToken(flag: string | undefined): string {
  const token = flag || process.env["GITHUB_TOKEN"] || (() => {
    try {
      return execSync("gh auth token", { encoding: "utf8" }).trim();
    } catch {
      return "";
    }
  })();
  if (!token) {
    console.error(
      "Error: no GitHub token found. Pass --token, set GITHUB_TOKEN, or run `gh auth login`."
    );
    process.exit(1);
  }
  return token;
}

export interface PlanIssueInput {
  issueNumber: number;
  api: GitHubApi;
}

/// Fetches the current issue state and computes the minimal label patch.
/// Makes no writes — safe to call at any time.
export async function planIssue({ issueNumber, api }: PlanIssueInput): Promise<Patch> {
  const issue = await api.getIssue(issueNumber);
  const currentLabels = new Set(issue.labels.map((l) => l.name));
  return computeIssuePatch({ issue, currentLabels });
}

/// Fetches, computes, and applies the label patch for a single issue.
/// Returns the patch that was applied (useful for tests and logging).
export async function syncIssue({ issueNumber, api }: PlanIssueInput): Promise<Patch> {
  const patch = await planIssue({ issueNumber, api });
  await applyPatch(patch, api);
  return patch;
}

/// Applies a Patch against the GitHub API.
/// Each target gets one batched addLabels call; each removal is individual.
export async function applyPatch(patch: Patch, api: GitHubApi): Promise<void> {
  for (const op of patch.ops) {
    const ref = chalk.cyan(`#${op.target}`);
    if (op.add.length > 0) {
      await api.addLabels(op.target, op.add);
      console.log(`${chalk.green("✔")} ${ref}: added [${chalk.green(op.add.join(", "))}]`);
    }
    for (const label of op.remove) {
      await api.removeLabel(op.target, label);
      console.log(`${chalk.red("✖")} ${ref}: removed "${chalk.red(label)}"`);
    }
    if (op.comment) {
      await api.addComment(op.target, op.comment);
      console.log(`${chalk.yellow("✉")} ${ref}: posted comment`);
    }
  }
}

/// Prints a human-readable plan to stdout without making any API calls.
export function printPlan(patch: Patch, subject: string): void {
  if (patch.ops.length === 0) {
    console.log(`${chalk.green("✔")} ${chalk.bold(subject)}: already in sync — no changes needed.`);
    return;
  }
  console.log(`${chalk.yellow("●")} ${chalk.bold(subject)}: plan (${chalk.bold(String(patch.ops.length))} target(s) to update)\n`);
  for (const op of patch.ops) {
    const title = op.title ? chalk.bold(` ${op.title}`) : "";
    const href  = op.url   ? `\n    ${chalk.dim(chalk.blue(op.url))}` : "";
    console.log(`  ${chalk.cyan(`#${op.target}`)}${title}${href}`);
    if (op.add.length > 0)
      console.log(`    ${chalk.green("+")} add:     ${chalk.green(op.add.join(", "))}`);
    if (op.remove.length > 0)
      console.log(`    ${chalk.red("-")} remove:  ${chalk.red(op.remove.join(", "))}`);
    if (op.comment)
      console.log(`    ${chalk.yellow("~")} comment: ${chalk.dim(op.comment.slice(0, 80))}…`);
  }
  console.log(`\n${chalk.dim("Run with --execute to apply.")}`);
}

// ---------------------------------------------------------------------------
// CLI entrypoint
// ---------------------------------------------------------------------------

if (process.argv[1] === url.fileURLToPath(import.meta.url)) {
  const argv = await yargs(hideBin(process.argv))
    .option("issue", { type: "number", demandOption: true, description: "Issue number" })
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
    const patch = await syncIssue({ issueNumber: argv.issue, api });
    if (patch.ops.length === 0) {
      console.log(`Issue #${argv.issue}: already in sync, no changes needed.`);
    }
  } else {
    const patch = await planIssue({ issueNumber: argv.issue, api });
    printPlan(patch, `Issue #${argv.issue}`);
  }
}
