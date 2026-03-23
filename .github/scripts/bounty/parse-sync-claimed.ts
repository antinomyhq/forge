#!/usr/bin/env tsx
// Parses an issues assigned/unassigned event and emits a ParsedIntent that
// adds or removes the `bounty: claimed` label on the issue.
//
// Usage:
//   tsx parse-sync-claimed.ts --issue <number> < event.json > intent.json

import * as fs from "fs";
import * as url from "url";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import type { IssuesEvent, ParsedIntent } from "./github-api.js";

const VALUE_PATTERN = /^bounty: \$/;
const CLAIMED_LABEL = "bounty: claimed";

export interface ParseSyncClaimedInput {
  issueNumber: number;
  event: IssuesEvent;
}

/// Parses the issues event and produces a ParsedIntent that adds `bounty: claimed`
/// on assignment or removes it when all assignees are gone.
/// Returns an empty intent if the issue has no bounty value label.
export function parse({ issueNumber, event }: ParseSyncClaimedInput): ParsedIntent {
  const { action, issue } = event;

  const hasBounty = issue.labels.some((l) => VALUE_PATTERN.test(l.name));
  if (!hasBounty) {
    return { mutations: [], knownLabels: {} };
  }

  const currentLabels = issue.labels.map((l) => l.name);

  if (action === "assigned") {
    return {
      mutations: [{ target: issueNumber, add: [CLAIMED_LABEL], remove: [] }],
      knownLabels: { [issueNumber]: currentLabels },
    };
  }

  if (action === "unassigned") {
    const remainingAssignees = issue.assignees ?? [];
    if (remainingAssignees.length === 0) {
      return {
        mutations: [{ target: issueNumber, add: [], remove: [CLAIMED_LABEL] }],
        knownLabels: { [issueNumber]: currentLabels },
      };
    }
  }

  // No-op (unassigned but assignees remain, or unknown action).
  return { mutations: [], knownLabels: {} };
}

// ---------------------------------------------------------------------------
// CLI entrypoint — only runs when executed directly (not imported by tests)
// ---------------------------------------------------------------------------

if (process.argv[1] === url.fileURLToPath(import.meta.url)) {
  const argv = await yargs(hideBin(process.argv))
    .option("issue", { type: "number", demandOption: true, description: "Issue number" })
    .strict()
    .parseAsync();

  const eventPath = process.env["GITHUB_EVENT_PATH"];
  if (!eventPath) {
    console.error("GITHUB_EVENT_PATH is not set");
    process.exit(1);
  }

  const event = JSON.parse(fs.readFileSync(eventPath, "utf8")) as IssuesEvent;
  const result = parse({ issueNumber: argv.issue, event });
  process.stdout.write(JSON.stringify(result, null, 2) + "\n");
}
