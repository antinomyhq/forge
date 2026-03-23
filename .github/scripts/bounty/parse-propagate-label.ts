#!/usr/bin/env tsx
// Parses a pull_request event and emits a ParsedIntent that copies bounty: $N
// labels from linked issues to the PR and posts a comment on each linked issue.
//
// Because issue labels are not in the PR event payload, this uses the
// `labelCopies` field — the plan step fetches issue labels and merges them.
//
// Usage:
//   tsx parse-propagate-label.ts --pr <number> < event.json > intent.json

import * as fs from "fs";
import * as url from "url";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import type { PullRequestEvent, ParsedIntent } from "./github-api.js";

const CLOSING_PATTERN = /(?:closes?|fixes?|resolves?)\s+#(\d+)/gi;

export interface ParsePropagateLabelInput {
  prNumber: number;
  event: PullRequestEvent;
}

/// Parses the PR event and produces a ParsedIntent that requests:
/// - Bounty label propagation from each linked issue to the PR (via labelCopies).
/// - A notification comment on each linked issue (resolved by the plan step
///   once it confirms the issue has bounty labels).
export function parse({ prNumber, event }: ParsePropagateLabelInput): ParsedIntent {
  const pr = event.pull_request;
  const body = pr.body ?? "";
  const prAuthor = pr.user.login;
  const prUrl = pr.html_url;

  const issueNumbers: number[] = [];
  let match: RegExpExecArray | null;
  while ((match = CLOSING_PATTERN.exec(body)) !== null) {
    issueNumbers.push(parseInt(match[1]!, 10));
  }

  if (issueNumbers.length === 0) {
    return { mutations: [], knownLabels: {} };
  }

  // One comment mutation per linked issue — the plan step drops it if the
  // issue turns out to have no bounty labels.
  const issueMutations = issueNumbers.map((n) => ({
    target: n,
    add: [] as string[],
    remove: [] as string[],
    comment: `PR ${prUrl} has been opened for this bounty by @${prAuthor}.`,
  }));

  return {
    mutations: issueMutations,
    // PR labels are known from the event — plan uses this to dedup adds.
    knownLabels: { [prNumber]: pr.labels.map((l) => l.name) },
    // The plan step fetches each source issue's labels and merges matching
    // bounty: $N ones into the PR target mutation.
    labelCopies: { sources: issueNumbers, prTarget: prNumber },
  };
}

// ---------------------------------------------------------------------------
// CLI entrypoint — only runs when executed directly (not imported by tests)
// ---------------------------------------------------------------------------

if (process.argv[1] === url.fileURLToPath(import.meta.url)) {
  const argv = await yargs(hideBin(process.argv))
    .option("pr", { type: "number", demandOption: true, description: "PR number" })
    .strict()
    .parseAsync();

  const eventPath = process.env["GITHUB_EVENT_PATH"];
  if (!eventPath) {
    console.error("GITHUB_EVENT_PATH is not set");
    process.exit(1);
  }

  const event = JSON.parse(fs.readFileSync(eventPath, "utf8")) as PullRequestEvent;
  const result = parse({ prNumber: argv.pr, event });
  process.stdout.write(JSON.stringify(result, null, 2) + "\n");
}
