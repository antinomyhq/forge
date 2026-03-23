#!/usr/bin/env tsx
// Keeps the generic `bounty` label in sync with the presence of any
// `bounty: 💰 $N` value label on an issue.
//
// - When a value label is **added**: also adds `bounty` (if not already present).
// - When a value label is **removed**: removes `bounty` only if no other value
//   labels remain on the issue.
//
// Usage:
//   tsx sync-generic-bounty.ts --issue <number> --repo <owner/repo> --token <token>

import * as fs from "fs";
import * as url from "url";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import {
  GitHubRestApi,
  type GitHubApi,
  type IssuesEvent,
} from "./github-api.js";

export interface SyncGenericBountyInput {
  issueNumber: number;
  event: IssuesEvent;
  api: GitHubApi;
}

const BOUNTY_LABEL = "bounty";
const VALUE_PATTERN = /^bounty: 💰 \$/;

/// Adds the generic `bounty` label when any value label is applied, and
/// removes it when the last value label is removed.
export async function run({
  issueNumber,
  event,
  api,
}: SyncGenericBountyInput): Promise<void> {
  const { action, issue } = event;

  // Only act on label events
  if (action !== "labeled" && action !== "unlabeled") {
    console.log(`Action "${action}" is not a label event, skipping.`);
    return;
  }

  // The label that was just added or removed — present on labeled/unlabeled events.
  const changedLabel = event.label?.name;

  if (!changedLabel || !VALUE_PATTERN.test(changedLabel)) {
    console.log(
      `Changed label "${changedLabel ?? "(none)"}" is not a bounty value label, skipping.`
    );
    return;
  }

  const currentLabels = issue.labels.map((l) => l.name);
  const hasGenericBounty = currentLabels.includes(BOUNTY_LABEL);
  const remainingValueLabels = currentLabels.filter((n) =>
    VALUE_PATTERN.test(n)
  );

  if (action === "labeled") {
    if (!hasGenericBounty) {
      await api.addLabels(issueNumber, [BOUNTY_LABEL]);
      console.log(`Added "${BOUNTY_LABEL}" to issue #${issueNumber}`);
    } else {
      console.log(
        `Issue #${issueNumber} already has label "${BOUNTY_LABEL}", skipping.`
      );
    }
  } else {
    // action === "unlabeled"
    // The removed label is already absent from issue.labels at this point.
    if (remainingValueLabels.length === 0 && hasGenericBounty) {
      await api.removeLabel(issueNumber, BOUNTY_LABEL);
      console.log(
        `Removed "${BOUNTY_LABEL}" from issue #${issueNumber} (no value labels remain)`
      );
    } else if (remainingValueLabels.length > 0) {
      console.log(
        `Issue #${issueNumber} still has ${remainingValueLabels.length} value label(s), keeping "${BOUNTY_LABEL}".`
      );
    } else {
      console.log(
        `Issue #${issueNumber} does not have "${BOUNTY_LABEL}", nothing to remove.`
      );
    }
  }
}

// ---------------------------------------------------------------------------
// CLI entrypoint — only runs when executed directly (not imported by tests)
// ---------------------------------------------------------------------------

if (process.argv[1] === url.fileURLToPath(import.meta.url)) {
  const argv = await yargs(hideBin(process.argv))
    .option("issue", {
      type: "number",
      demandOption: true,
      description: "Issue number",
    })
    .option("repo", {
      type: "string",
      demandOption: true,
      description: "owner/repo",
    })
    .option("token", {
      type: "string",
      demandOption: true,
      description: "GitHub token",
    })
    .strict()
    .parseAsync();

  const eventPath = process.env["GITHUB_EVENT_PATH"];
  if (!eventPath) {
    console.error("GITHUB_EVENT_PATH is not set");
    process.exit(1);
  }

  const event = JSON.parse(
    fs.readFileSync(eventPath, "utf8")
  ) as IssuesEvent;
  const [owner, repo] = argv.repo.split("/") as [string, string];

  await run({
    issueNumber: argv.issue,
    event,
    api: new GitHubRestApi(owner, repo, argv.token),
  });
}
