#!/usr/bin/env tsx
// Orchestrator — runs the full parse → plan → execute pipeline in a single
// Node process with no temp files. Each step passes data directly to the next
// as in-memory JavaScript objects.
//
// Usage:
//   tsx run.ts --script <parse-script> --repo <owner/repo> --token <token> [--pr N | --issue N]

import * as fs from "fs";
import * as url from "url";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import { GitHubRestApi, type PullRequestEvent, type IssuesEvent } from "./github-api.js";
import { plan } from "./plan.js";
import { execute } from "./execute.js";

/// Supported parse scripts, each with its event type and required arg.
const SCRIPTS = {
  "parse-propagate-label": { eventType: "pr" as const },
  "parse-sync-claimed": { eventType: "issue" as const },
  "parse-sync-generic-bounty": { eventType: "issue" as const },
  "parse-mark-rewarded": { eventType: "pr" as const },
} as const;

type ScriptName = keyof typeof SCRIPTS;

if (process.argv[1] === url.fileURLToPath(import.meta.url)) {
  const argv = await yargs(hideBin(process.argv))
    .option("script", {
      type: "string",
      demandOption: true,
      choices: Object.keys(SCRIPTS) as ScriptName[],
      description: "Which parse script to run",
    })
    .option("repo", { type: "string", demandOption: true, description: "owner/repo" })
    .option("token", { type: "string", demandOption: true, description: "GitHub token" })
    .option("pr", { type: "number", description: "PR number (for PR-based scripts)" })
    .option("issue", { type: "number", description: "Issue number (for issue-based scripts)" })
    .strict()
    .parseAsync();

  const scriptName = argv.script as ScriptName;
  const { eventType } = SCRIPTS[scriptName];

  const eventPath = process.env["GITHUB_EVENT_PATH"];
  if (!eventPath) {
    console.error("GITHUB_EVENT_PATH is not set");
    process.exit(1);
  }

  const rawEvent = JSON.parse(fs.readFileSync(eventPath, "utf8"));
  const [owner, repo] = argv.repo.split("/") as [string, string];
  const api = new GitHubRestApi(owner, repo, argv.token);

  // Dynamically import the parse module so run.ts stays generic.
  const parseModule = await import(`./${scriptName}.js`);

  let intent;
  if (eventType === "pr") {
    if (argv.pr === undefined) {
      console.error(`--pr is required for ${scriptName}`);
      process.exit(1);
    }
    intent = parseModule.parse({ prNumber: argv.pr, event: rawEvent as PullRequestEvent });
  } else {
    if (argv.issue === undefined) {
      console.error(`--issue is required for ${scriptName}`);
      process.exit(1);
    }
    intent = parseModule.parse({ issueNumber: argv.issue, event: rawEvent as IssuesEvent });
  }

  const batchPlan = await plan({ intent, api });
  await execute({ plan: batchPlan, api });
}
