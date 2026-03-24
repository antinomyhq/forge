import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { planAllIssues, syncAllIssues } from "../src/sync-all-issues.js";
import type { GitHubApi } from "../src/api.js";
import type { Issue, PullRequest } from "../src/types.js";

// ---------------------------------------------------------------------------
// Mock API
// ---------------------------------------------------------------------------

function makeMockApi(issues: Issue[]): GitHubApi & {
  added: Map<number, string[][]>;
  removed: Map<number, string[]>;
} {
  const added = new Map<number, string[][]>();
  const removed = new Map<number, string[]>();

  return {
    added,
    removed,
    async getIssue(number) {
      const found = issues.find((i) => i.number === number);
      if (!found) throw new Error(`Issue #${number} not found in mock`);
      return found;
    },
    async getPullRequest(): Promise<PullRequest> {
      throw new Error("not used");
    },
    async listIssuesWithLabelPrefix(_prefix) {
      // Return only real issues (not PRs — pull_request field absent)
      return issues.filter((i) => i.pull_request === undefined);
    },
    async addLabels(target, labels) {
      if (!added.has(target)) added.set(target, []);
      added.get(target)!.push(labels);
    },
    async removeLabel(target, label) {
      if (!removed.has(target)) removed.set(target, []);
      removed.get(target)!.push(label);
    },
    async addComment() {},
  };
}

function makeIssue(overrides: Partial<Issue> & { number: number }): Issue {
  return { title: "Test issue", html_url: `https://github.com/owner/repo/issues/${overrides.number}`, state: "open", labels: [], assignees: [], ...overrides };
}

function labelNames(...names: string[]): { name: string }[] {
  return names.map((name) => ({ name }));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("planAllIssues", () => {
  it("returns empty patch when no issues need changes", async () => {
    const issues = [
      makeIssue({ number: 1, labels: labelNames("bounty: $100", "bounty") }),
      makeIssue({ number: 2, labels: labelNames("type: bug") }),
    ];
    const api = makeMockApi(issues);
    const patch = await planAllIssues({ api });
    assert.deepEqual(patch.ops, []);
  });

  it("plans adds for issues missing the generic bounty label", async () => {
    const issues = [
      makeIssue({ number: 1, labels: labelNames("bounty: $100") }),
      makeIssue({ number: 2, labels: labelNames("bounty: $200") }),
    ];
    const api = makeMockApi(issues);
    const patch = await planAllIssues({ api });
    assert.equal(patch.ops.length, 2);
    assert.ok(patch.ops.every((op) => op.add.includes("bounty")));
  });

  it("plans removal of stale claimed label when issue is unassigned", async () => {
    const issues = [
      makeIssue({
        number: 3,
        labels: labelNames("bounty: $500", "bounty", "bounty: claimed"),
        assignees: [],
      }),
    ];
    const api = makeMockApi(issues);
    const patch = await planAllIssues({ api });
    assert.equal(patch.ops.length, 1);
    assert.deepEqual(patch.ops[0]!.remove, ["bounty: claimed"]);
  });

  it("collects ops for multiple issues with different needs", async () => {
    const issues = [
      makeIssue({ number: 1, labels: labelNames("bounty: $100") }), // missing generic
      makeIssue({ number: 2, labels: labelNames("bounty: $200", "bounty") }), // already in sync
      makeIssue({
        number: 3,
        labels: labelNames("bounty: $300", "bounty", "bounty: claimed"),
        assignees: [],
      }), // stale claimed
    ];
    const api = makeMockApi(issues);
    const patch = await planAllIssues({ api });
    assert.equal(patch.ops.length, 2);
    const targets = patch.ops.map((op) => op.target).sort();
    assert.deepEqual(targets, [1, 3]);
  });
});

describe("syncAllIssues", () => {
  it("applies adds and removals across all issues", async () => {
    const issues = [
      makeIssue({ number: 1, labels: labelNames("bounty: $100") }),
      makeIssue({
        number: 2,
        labels: labelNames("bounty: $200", "bounty", "bounty: claimed"),
        assignees: [],
      }),
    ];
    const api = makeMockApi(issues);
    await syncAllIssues({ api });

    assert.ok(api.added.get(1)?.[0]?.includes("bounty"));
    assert.deepEqual(api.removed.get(2), ["bounty: claimed"]);
  });

  it("returns empty patch when all issues are already in sync", async () => {
    const issues = [
      makeIssue({ number: 1, labels: labelNames("bounty: $100", "bounty") }),
    ];
    const api = makeMockApi(issues);
    const patch = await syncAllIssues({ api });
    assert.deepEqual(patch.ops, []);
    assert.equal(api.added.size, 0);
    assert.equal(api.removed.size, 0);
  });
});
