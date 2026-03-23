// Converts a ParsedIntent into a BatchPlan by fetching current label state
// for any targets not already known from the event payload, then diffing
// desired vs actual to eliminate no-op operations.
//
// Usage (reads intent JSON from INTENT_FILE env var or stdin):
//   INTENT_FILE=intent.json tsx plan.ts --repo <owner/repo> --token <token>

import * as fs from "fs";
import * as url from "url";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import {
  GitHubRestApi,
  type GitHubApi,
  type ParsedIntent,
  type BatchPlan,
  type TargetMutation,
} from "./github-api.js";

const VALUE_PATTERN = /^bounty: \$/;

export interface PlanInput {
  intent: ParsedIntent;
  api: GitHubApi;
}

/// Resolves a ParsedIntent into a BatchPlan:
/// 1. Resolves any `labelCopies` by fetching source issue labels and merging
///    matching bounty: $N labels into the PR target mutation.
/// 2. For each mutation target, fetches current labels when not in `knownLabels`.
/// 3. Filters out no-op adds and removes.
export async function plan({ intent, api }: PlanInput): Promise<BatchPlan> {
  // Mutable map of target → desired mutation so we can merge labelCopies.
  const mutationMap = new Map<number, TargetMutation>(
    intent.mutations.map((m) => [m.target, { ...m, add: [...m.add], remove: [...m.remove] }])
  );

  // Step 1 — resolve labelCopies: fetch each source issue's labels, collect
  // any bounty: $N labels, and merge them into the PR target mutation.
  if (intent.labelCopies) {
    const { sources, prTarget } = intent.labelCopies;

    if (!mutationMap.has(prTarget)) {
      mutationMap.set(prTarget, { target: prTarget, add: [], remove: [] });
    }
    const prMutation = mutationMap.get(prTarget)!;

    for (const issueNumber of sources) {
      let issueLabels: string[];
      try {
        const fetched = await api.getLabels(issueNumber);
        issueLabels = fetched.map((l) => l.name);
      } catch (e) {
        console.error(`Could not fetch labels for issue #${issueNumber}: ${String(e)}, skipping.`);
        // Drop the comment mutation for this issue too.
        mutationMap.delete(issueNumber);
        continue;
      }

      const bountyLabels = issueLabels.filter((n) => VALUE_PATTERN.test(n));

      // Merge bounty labels into the PR add list.
      for (const label of bountyLabels) {
        if (!prMutation.add.includes(label)) {
          prMutation.add.push(label);
        }
      }

      // If the issue has no bounty labels, drop the comment — nothing to notify.
      if (bountyLabels.length === 0 && mutationMap.has(issueNumber)) {
        mutationMap.delete(issueNumber);
      }
    }
  }

  // Step 2 — for each mutation, fetch current labels if not already known,
  // then filter out no-ops.
  const resolved: TargetMutation[] = [];

  for (const mutation of mutationMap.values()) {
    const { target, add, remove, comment } = mutation;

    let currentLabels: Set<string>;
    const known = intent.knownLabels[target];
    if (known !== undefined) {
      currentLabels = new Set(known);
    } else {
      try {
        const fetched = await api.getLabels(target);
        currentLabels = new Set(fetched.map((l) => l.name));
      } catch (e) {
        console.error(`Could not fetch labels for #${target}: ${String(e)}, skipping.`);
        continue;
      }
    }

    const toAdd = add.filter((l) => !currentLabels.has(l));
    const toRemove = remove.filter((l) => currentLabels.has(l));

    if (toAdd.length === 0 && toRemove.length === 0 && !comment) continue;

    resolved.push({ target, add: toAdd, remove: toRemove, comment });
  }

  return { mutations: resolved };
}

// ---------------------------------------------------------------------------
// CLI entrypoint — only runs when executed directly (not imported by tests)
// ---------------------------------------------------------------------------

if (process.argv[1] === url.fileURLToPath(import.meta.url)) {
  const argv = await yargs(hideBin(process.argv))
    .option("repo", { type: "string", demandOption: true, description: "owner/repo" })
    .option("token", { type: "string", demandOption: true, description: "GitHub token" })
    .strict()
    .parseAsync();

  const intentJson = fs.readFileSync(process.env["INTENT_FILE"] ?? "/dev/stdin", "utf8");
  const intent = JSON.parse(intentJson) as ParsedIntent;
  const [owner, repo] = argv.repo.split("/") as [string, string];

  const result = await plan({ intent, api: new GitHubRestApi(owner, repo, argv.token) });
  process.stdout.write(JSON.stringify(result, null, 2) + "\n");
}
