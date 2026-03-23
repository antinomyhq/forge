#!/usr/bin/env tsx
// Parses an issues labeled/unlabeled event and emits a ParsedIntent that keeps
// the generic `bounty` label in sync with bounty: $N value labels.
//
// Usage:
//   tsx parse-sync-generic-bounty.ts --issue <number> < event.json > intent.json

import * as fs from "fs";
import * as url from "url";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import type { IssuesEvent, ParsedIntent } from "./github-api.js";

const VALUE_PATTERN = /^bounty: \$/;
const BOUNTY_LABEL = "bounty";

export interface ParseSyncGenericBountyInput {
  issueNumber: number;
  event: IssuesEvent;
}

/// Parses the label event and produces a ParsedIntent that adds the generic
/// `bounty` label when a value label is applied, or removes it when the last
/// value label is removed.
export function parse({ issueNumber, event }: ParseSyncGenericBountyInput): ParsedIntent {
  const { action, issue } = event;

  if (action !== "labeled" && action !== "unlabeled") {
    return { mutations: [], knownLabels: {} };
  }

  const changedLabel = event.label?.name;
  if (!changedLabel || !VALUE_PATTERN.test(changedLabel)) {
    return { mutations: [], knownLabels: {} };
  }

  const currentLabels = issue.labels.map((l) => l.name);
  // issue.labels reflects state after the event action.
  const remainingValueLabels = currentLabels.filter((n) => VALUE_PATTERN.test(n));

  if (action === "labeled") {
    return {
      mutations: [{ target: issueNumber, add: [BOUNTY_LABEL], remove: [] }],
      knownLabels: { [issueNumber]: currentLabels },
    };
  }

  // action === "unlabeled": only remove generic label when no value labels remain.
  if (remainingValueLabels.length === 0) {
    return {
      mutations: [{ target: issueNumber, add: [], remove: [BOUNTY_LABEL] }],
      knownLabels: { [issueNumber]: currentLabels },
    };
  }

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
