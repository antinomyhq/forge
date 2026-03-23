#!/usr/bin/env tsx
// Parses a pull_request_target closed event and emits a ParsedIntent that
// adds `bounty: rewarded` to the merged PR and linked issues, and removes
// `bounty: claimed` from those issues.
//
// Usage:
//   tsx parse-mark-rewarded.ts --pr <number> < event.json > intent.json

import * as fs from "fs";
import * as url from "url";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import type { PullRequestEvent, ParsedIntent, TargetMutation } from "./github-api.js";

const VALUE_PATTERN = /^bounty: \$/;
const CLOSING_PATTERN = /(?:closes?|fixes?|resolves?)\s+#(\d+)/gi;
const REWARDED_LABEL = "bounty: rewarded";
const CLAIMED_LABEL = "bounty: claimed";

export interface ParseMarkRewardedInput {
  prNumber: number;
  event: PullRequestEvent;
}

/// Parses the PR close event and produces a ParsedIntent that applies
/// `bounty: rewarded` to the PR and all linked issues, and removes
/// `bounty: claimed` from those issues.
/// Returns an empty intent if the PR was not merged or has no bounty label.
export function parse({ prNumber, event }: ParseMarkRewardedInput): ParsedIntent {
  const pr = event.pull_request;

  if (!pr.merged) {
    return { mutations: [], knownLabels: {} };
  }

  const prLabelNames = pr.labels.map((l) => l.name);
  const hasBounty = prLabelNames.some((n) => VALUE_PATTERN.test(n));
  if (!hasBounty) {
    return { mutations: [], knownLabels: {} };
  }

  const mutations: TargetMutation[] = [];
  const knownLabels: Record<number, string[]> = {};

  // PR mutation — add rewarded. PR labels are known from the event.
  mutations.push({ target: prNumber, add: [REWARDED_LABEL], remove: [] });
  knownLabels[prNumber] = prLabelNames;

  // Linked issue mutations — add rewarded, remove claimed.
  // Issue labels are NOT in the event, so they're not included in knownLabels;
  // the plan step will fetch them.
  const body = pr.body ?? "";
  let match: RegExpExecArray | null;
  while ((match = CLOSING_PATTERN.exec(body)) !== null) {
    const issueNumber = parseInt(match[1]!, 10);
    mutations.push({ target: issueNumber, add: [REWARDED_LABEL], remove: [CLAIMED_LABEL] });
  }

  return { mutations, knownLabels };
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
