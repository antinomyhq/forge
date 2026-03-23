#!/usr/bin/env tsx
// Copies bounty USD value labels from a linked issue to the PR that references
// it via closing keywords, and posts a notification comment on the issue.
//
// Usage:
//   tsx propagate-label.ts --pr <number> --repo <owner/repo> --token <token>

import * as fs from "fs";
import * as url from "url";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import {
  GitHubRestApi,
  type GitHubApi,
  type PullRequestEvent,
} from "./github-api.js";

export interface PropagateInput {
  prNumber: number;
  event: PullRequestEvent;
  api: GitHubApi;
}

/// Copies bounty value labels from any linked issues to the PR and posts
/// a notification comment on each linked issue that has a bounty label.
export async function run({ prNumber, event, api }: PropagateInput): Promise<void> {
  const pr = event.pull_request;
  const body = pr.body ?? "";
  const prAuthor = pr.user.login;
  const prUrl = pr.html_url;

  // Extract issue numbers from closing keywords
  const closingPattern = /(?:closes?|fixes?|resolves?)\s+#(\d+)/gi;
  const issueNumbers: number[] = [];
  let match: RegExpExecArray | null;
  while ((match = closingPattern.exec(body)) !== null) {
    issueNumbers.push(parseInt(match[1]!, 10));
  }

  if (issueNumbers.length === 0) {
    console.log("No linked issues found in PR body.");
    return;
  }

  const existingPrLabels = await api.getLabels(prNumber);
  const existingPrLabelNames = new Set(existingPrLabels.map((l) => l.name));

  for (const issueNumber of issueNumbers) {
    let issueLabels;
    try {
      issueLabels = await api.getLabels(issueNumber);
    } catch (e) {
      console.log(`Could not fetch labels for issue #${issueNumber}: ${String(e)}`);
      continue;
    }

    const bountyLabels = issueLabels
      .map((l) => l.name)
      .filter((name) => /^bounty: 💰 \$/.test(name));

    for (const label of bountyLabels) {
      if (!existingPrLabelNames.has(label)) {
        await api.addLabels(prNumber, [label]);
        console.log(`Added "${label}" to PR #${prNumber} from issue #${issueNumber}`);
      } else {
        console.log(`PR #${prNumber} already has label "${label}"`);
      }
    }

    if (bountyLabels.length > 0) {
      await api.addComment(
        issueNumber,
        `PR ${prUrl} has been opened for this bounty by @${prAuthor}.`
      );
      console.log(`Notified issue #${issueNumber} about PR #${prNumber}`);
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
