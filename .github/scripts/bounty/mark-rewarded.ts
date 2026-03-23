#!/usr/bin/env tsx
// Applies `bounty: rewarded` to the merged PR and all linked issues, and
// removes `bounty: claimed` from those issues.
//
// Usage:
//   tsx mark-rewarded.ts --pr <number> --repo <owner/repo> --token <token>

import * as fs from "fs";
import * as url from "url";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import {
  GitHubRestApi,
  type GitHubApi,
  type PullRequestEvent,
} from "./github-api.js";

export interface MarkRewardedInput {
  prNumber: number;
  event: PullRequestEvent;
  api: GitHubApi;
}

/// Marks the merged PR and all linked issues as rewarded. Removes
/// `bounty: claimed` from each linked issue once the bounty is paid out.
export async function run({ prNumber, event, api }: MarkRewardedInput): Promise<void> {
  const pr = event.pull_request;

  if (!pr.merged) {
    console.log(`PR #${prNumber} was closed but not merged, skipping.`);
    return;
  }

  const prLabelNames = new Set(pr.labels.map((l) => l.name));
  const hasBounty = [...prLabelNames].some((name) => /^bounty: 💰 \$/.test(name));
  if (!hasBounty) {
    console.log(`PR #${prNumber} has no bounty label, skipping.`);
    return;
  }

  const REWARDED_LABEL = "bounty: rewarded";
  const CLAIMED_LABEL = "bounty: claimed";

  if (!prLabelNames.has(REWARDED_LABEL)) {
    await api.addLabels(prNumber, [REWARDED_LABEL]);
    console.log(`Added "${REWARDED_LABEL}" to PR #${prNumber}`);
  }

  const body = pr.body ?? "";
  const closingPattern = /(?:closes?|fixes?|resolves?)\s+#(\d+)/gi;
  let match: RegExpExecArray | null;

  while ((match = closingPattern.exec(body)) !== null) {
    const issueNumber = parseInt(match[1]!, 10);

    let issueLabels;
    try {
      issueLabels = await api.getLabels(issueNumber);
    } catch (e) {
      console.log(`Could not fetch labels for issue #${issueNumber}: ${String(e)}`);
      continue;
    }

    const issueLabelNames = new Set(issueLabels.map((l) => l.name));

    if (!issueLabelNames.has(REWARDED_LABEL)) {
      await api.addLabels(issueNumber, [REWARDED_LABEL]);
      console.log(`Added "${REWARDED_LABEL}" to issue #${issueNumber}`);
    }

    if (issueLabelNames.has(CLAIMED_LABEL)) {
      await api.removeLabel(issueNumber, CLAIMED_LABEL);
      console.log(`Removed "${CLAIMED_LABEL}" from issue #${issueNumber}`);
    }
  }
}

// ---------------------------------------------------------------------------
// CLI entrypoint — only runs when executed directly (not imported by tests)
// ---------------------------------------------------------------------------

if (process.argv[1] === url.fileURLToPath(import.meta.url)) {
  const argv = await yargs(hideBin(process.argv))
    .option("pr", { type: "number", demandOption: true, description: "PR number" })
    .option("repo", { type: "string", demandOption: true, description: "owner/repo" })
    .option("token", { type: "string", demandOption: true, description: "GitHub token" })
    .strict()
    .parseAsync();

  const eventPath = process.env["GITHUB_EVENT_PATH"];
  if (!eventPath) {
    console.error("GITHUB_EVENT_PATH is not set");
    process.exit(1);
  }

  const event = JSON.parse(fs.readFileSync(eventPath, "utf8")) as PullRequestEvent;
  const [owner, repo] = argv.repo.split("/") as [string, string];

  await run({
    prNumber: argv.pr,
    event,
    api: new GitHubRestApi(owner, repo, argv.token),
  });
}
