import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { planAllPrs, syncAllPrs } from "../src/sync-all-prs.js";
import type { GitHubApi } from "../src/api.js";
import type { Issue, PullRequest } from "../src/types.js";
import { BOUNTY_CLAIMED, BOUNTY_REWARDED } from "../src/rules.js";

// ---------------------------------------------------------------------------
// Mock API
// ---------------------------------------------------------------------------

function makeMockApi(prs: PullRequest[], issues: Issue[]): GitHubApi & {
  added: Map<number, string[][]>;
  removed: Map<number, string[]>;
  comments: Map<number, string[]>;
} {
  const added = new Map<number, string[][]>();
  const removed = new Map<number, string[]>();
  const comments = new Map<number, string[]>();

  return {
    added,
    removed,
    comments,
    async getIssue(number) {
      const found = issues.find((i) => i.number === number);
      if (!found) throw new Error(`Issue #${number} not found in mock`);
      return found;
    },
    async getPullRequest(number) {
      const found = prs.find((p) => p.number === number);
      if (!found) throw new Error(`PR #${number} not found in mock`);
      return found;
    },
    async listIssuesWithLabelPrefix(): Promise<Issue[]> {
      throw new Error("not used");
    },
    async listPrsWithLabelPrefix(_prefix) {
      return prs;
    },
    async addLabels(target, labels) {
      if (!added.has(target)) added.set(target, []);
      added.get(target)!.push(labels);
    },
    async removeLabel(target, label) {
      if (!removed.has(target)) removed.set(target, []);
      removed.get(target)!.push(label);
    },
    async addComment(target, body) {
      if (!comments.has(target)) comments.set(target, []);
      comments.get(target)!.push(body);
    },
  };
}

function makeIssue(overrides: Partial<Issue> & { number: number }): Issue {
  return {
    title: "Test issue",
    html_url: `https://github.com/owner/repo/issues/${overrides.number}`,
    state: "open",
    labels: [],
    assignees: [],
    ...overrides,
  };
}

function makePr(overrides: Partial<PullRequest> & { number: number }): PullRequest {
  return {
    title: "Test PR",
    state: "open",
    merged: false,
    body: null,
    labels: [],
    user: { login: "dev" },
    html_url: `https://github.com/owner/repo/pull/${overrides.number}`,
    ...overrides,
  };
}

function labelNames(...names: string[]): { name: string }[] {
  return names.map((name) => ({ name }));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("planAllPrs", () => {
  it("returns empty patch when no PRs match", async () => {
    const api = makeMockApi([], []);
    const patch = await planAllPrs({ api });
    assert.deepEqual(patch.ops, []);
  });

  it("returns empty patch when PR already has the propagated label", async () => {
    const pr = makePr({ number: 10, body: "Closes #1", labels: labelNames("bounty: 💰 $500") });
    const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $500") });
    const api = makeMockApi([pr], [issue]);
    const patch = await planAllPrs({ api });
    assert.deepEqual(patch.ops, []);
  });

  it("plans propagation of value label from linked issue to PR", async () => {
    const pr = makePr({ number: 10, body: "Closes #1" });
    const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $300") });
    const api = makeMockApi([pr], [issue]);
    const patch = await planAllPrs({ api });
    const prOp = patch.ops.find((op) => op.target === 10);
    assert.ok(prOp?.add.includes("bounty: 💰 $300"));
  });

  it("plans rewarded lifecycle for merged PR", async () => {
    const pr = makePr({
      number: 10,
      merged: true,
      body: "Closes #1",
      labels: labelNames("bounty: 💰 $500"),
    });
    const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $500", BOUNTY_CLAIMED) });
    const api = makeMockApi([pr], [issue]);
    const patch = await planAllPrs({ api });

    const prOp = patch.ops.find((op) => op.target === 10);
    const issueOp = patch.ops.find((op) => op.target === 1);
    assert.ok(prOp?.add.includes(BOUNTY_REWARDED));
    assert.ok(issueOp?.add.includes(BOUNTY_REWARDED));
    assert.ok(issueOp?.remove.includes(BOUNTY_CLAIMED));
  });

  it("processes multiple PRs in one pass", async () => {
    const pr1 = makePr({ number: 10, body: "Closes #1" });
    const pr2 = makePr({ number: 20, body: "Closes #2" });
    const issue1 = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $100") });
    const issue2 = makeIssue({ number: 2, labels: labelNames("bounty: 💰 $200") });
    const api = makeMockApi([pr1, pr2], [issue1, issue2]);
    const patch = await planAllPrs({ api });

    const targets = patch.ops.map((op) => op.target).sort((a, b) => a - b);
    // Each PR gets a label-add op; each issue gets a comment op
    assert.ok(targets.includes(10));
    assert.ok(targets.includes(20));
  });

  it("skips a linked issue that fails to fetch without aborting", async () => {
    const pr = makePr({ number: 10, body: "Closes #999" });
    // #999 not in mock — will throw
    const api = makeMockApi([pr], []);
    const patch = await planAllPrs({ api });
    // No crash; patch may be empty since the only linked issue errored
    assert.ok(Array.isArray(patch.ops));
  });
});

describe("syncAllPrs", () => {
  it("applies label propagation across all PRs", async () => {
    const pr = makePr({ number: 10, body: "Closes #1" });
    const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $500") });
    const api = makeMockApi([pr], [issue]);
    await syncAllPrs({ api });

    assert.ok(api.added.get(10)?.[0]?.includes("bounty: 💰 $500"));
  });

  it("returns empty patch when all PRs are already in sync", async () => {
    const pr = makePr({ number: 10, body: "Closes #1", labels: labelNames("bounty: 💰 $500") });
    const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $500") });
    const api = makeMockApi([pr], [issue]);
    const patch = await syncAllPrs({ api });

    assert.deepEqual(patch.ops.filter((op) => op.add.length > 0 || op.remove.length > 0), []);
    assert.equal(api.added.size, 0);
    assert.equal(api.removed.size, 0);
  });
});
