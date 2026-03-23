#!/usr/bin/env tsx
// Adds `bounty: claimed` to a bounty issue when it is assigned, and removes it
// when all assignees are unassigned.
//
// Usage:
//   tsx sync-claimed.ts --issue <number> --repo <owner/repo> --token <token>

import * as fs from "fs";
import * as url from "url";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import {
  GitHubRestApi,
  type GitHubApi,
  type IssuesEvent,
} from "./github-api.js";

export interface SyncClaimedInput {
  issueNumber: number;
  event: IssuesEvent;
  api: GitHubApi;
}

/// Adds `bounty: claimed` when a bounty issue is assigned. Removes it only
/// when the last assignee is removed from the issue.
export async function run({ issueNumber, event, api }: SyncClaimedInput): Promise<void> {
  const { action, issue } = event;

  const hasBounty = issue.labels.some((l) => /^bounty: 💰 \$/.test(l.name));
  if (!hasBounty) {
    console.log(`Issue #${issueNumber} has no bounty label, skipping.`);
    return;
  }

  const CLAIMED_LABEL = "bounty: claimed";
  const currentLabels = new Set(issue.labels.map((l) => l.name));

  if (action === "assigned") {
    if (!currentLabels.has(CLAIMED_LABEL)) {
      await api.addLabels(issueNumber, [CLAIMED_LABEL]);
      console.log(`Added "${CLAIMED_LABEL}" to issue #${issueNumber}`);
    } else {
      console.log(`Issue #${issueNumber} already has label "${CLAIMED_LABEL}"`);
    }
  } else if (action === "unassigned") {
    const remainingAssignees = issue.assignees ?? [];
    if (remainingAssignees.length === 0 && currentLabels.has(CLAIMED_LABEL)) {
      await api.removeLabel(issueNumber, CLAIMED_LABEL);
      console.log(`Removed "${CLAIMED_LABEL}" from issue #${issueNumber} (no assignees left)`);
    } else {
      console.log(
        `Issue #${issueNumber} still has ${remainingAssignees.length} assignee(s), keeping "${CLAIMED_LABEL}"`
      );
    }
  }
}

// ---------------------------------------------------------------------------
// CLI entrypoint — only runs when executed directly (not imported by tests)
// ---------------------------------------------------------------------------

if (process.argv[1] === url.fileURLToPath(import.meta.url)) {
  const argv = await yargs(hideBin(process.argv))
    .option("issue", { type: "number", demandOption: true, description: "Issue number" })
    .option("repo", { type: "string", demandOption: true, description: "owner/repo" })
    .option("token", { type: "string", demandOption: true, description: "GitHub token" })
    .strict()
    .parseAsync();

  const eventPath = process.env["GITHUB_EVENT_PATH"];
  if (!eventPath) {
    console.error("GITHUB_EVENT_PATH is not set");
    process.exit(1);
  }

  const event = JSON.parse(fs.readFileSync(eventPath, "utf8")) as IssuesEvent;
  const [owner, repo] = argv.repo.split("/") as [string, string];

  await run({
    issueNumber: argv.issue,
    event,
    api: new GitHubRestApi(owner, repo, argv.token),
  });
}
