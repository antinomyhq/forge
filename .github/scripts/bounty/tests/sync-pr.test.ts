import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { syncPr } from "../src/sync-pr.js";
import type { GitHubApi } from "../src/api.js";
import type { Issue, PullRequest } from "../src/types.js";
import { BOUNTY_CLAIMED, BOUNTY_REWARDED } from "../src/rules.js";

// ---------------------------------------------------------------------------
// Mock API
// ---------------------------------------------------------------------------

interface MockApi extends GitHubApi {
  addedByTarget: Map<number, string[][]>;
  removedByTarget: Map<number, string[]>;
  commentsByTarget: Map<number, string[]>;
}

function makeMockApi(pr: PullRequest, issues: Issue[]): MockApi {
  const addedByTarget = new Map<number, string[][]>();
  const removedByTarget = new Map<number, string[]>();
  const commentsByTarget = new Map<number, string[]>();

  function recordAdd(target: number, labels: string[]) {
    if (!addedByTarget.has(target)) addedByTarget.set(target, []);
    addedByTarget.get(target)!.push(labels);
  }
  function recordRemove(target: number, label: string) {
    if (!removedByTarget.has(target)) removedByTarget.set(target, []);
    removedByTarget.get(target)!.push(label);
  }
  function recordComment(target: number, body: string) {
    if (!commentsByTarget.has(target)) commentsByTarget.set(target, []);
    commentsByTarget.get(target)!.push(body);
  }

  return {
    addedByTarget,
    removedByTarget,
    commentsByTarget,
    async getIssue(number) {
      const found = issues.find((i) => i.number === number);
      if (!found) throw new Error(`Issue #${number} not found in mock`);
      return found;
    },
    async getPullRequest() {
      return pr;
    },
    async listIssuesWithLabelPrefix(): Promise<Issue[]> {
      throw new Error("not used");
    },
    async addLabels(target, labels) {
      recordAdd(target, labels);
    },
    async removeLabel(target, label) {
      recordRemove(target, label);
    },
    async addComment(target, body) {
      recordComment(target, body);
    },
  };
}

function makeIssue(overrides: Partial<Issue> & { number: number }): Issue {
  return { title: "issue", html_url: `https://github.com/owner/repo/issues/${overrides.number}`, state: "open", labels: [], assignees: [], ...overrides };
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

describe("syncPr", () => {
  describe("label propagation", () => {
    it("returns empty patch when PR has no linked issues", async () => {
      const pr = makePr({ number: 10, body: "No links" });
      const api = makeMockApi(pr, []);
      const patch = await syncPr({ prNumber: 10, api });
      assert.deepEqual(patch.ops, []);
    });

    it("returns empty patch when linked issue has no bounty label", async () => {
      const pr = makePr({ number: 10, body: "Closes #1" });
      const issue = makeIssue({ number: 1, labels: labelNames("type: bug") });
      const api = makeMockApi(pr, [issue]);
      const patch = await syncPr({ prNumber: 10, api });
      assert.deepEqual(patch.ops, []);
    });

    it("copies bounty label from linked issue to PR", async () => {
      const pr = makePr({ number: 10, body: "Closes #1" });
      const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $500") });
      const api = makeMockApi(pr, [issue]);
      await syncPr({ prNumber: 10, api });

      assert.ok(api.addedByTarget.get(10)?.[0]?.includes("bounty: 💰 $500"));
    });

    it("skips label already on the PR", async () => {
      const pr = makePr({
        number: 10,
        body: "Closes #1",
        labels: labelNames("bounty: 💰 $500"),
      });
      const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $500") });
      const api = makeMockApi(pr, [issue]);
      const patch = await syncPr({ prNumber: 10, api });
      assert.deepEqual(patch.ops, []);
    });

    it("posts comment on linked issue when label is propagated", async () => {
      const pr = makePr({ number: 10, body: "Closes #1" });
      const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $200") });
      const api = makeMockApi(pr, [issue]);
      await syncPr({ prNumber: 10, api });

      const comments = api.commentsByTarget.get(1);
      assert.ok(comments && comments.length === 1);
      assert.ok(comments[0]!.includes("#10"));
      assert.ok(comments[0]!.includes("dev"));
    });

    it("does not post comment when label was already on the PR", async () => {
      const pr = makePr({
        number: 10,
        body: "Closes #1",
        labels: labelNames("bounty: 💰 $200"),
      });
      const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $200") });
      const api = makeMockApi(pr, [issue]);
      await syncPr({ prNumber: 10, api });

      assert.ok(!api.commentsByTarget.has(1));
    });

    it("fetches multiple linked issues in parallel and merges labels", async () => {
      const pr = makePr({ number: 10, body: "Closes #1\nCloses #2" });
      const issue1 = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $100") });
      const issue2 = makeIssue({ number: 2, labels: labelNames("bounty: 💰 $200") });
      const api = makeMockApi(pr, [issue1, issue2]);
      await syncPr({ prNumber: 10, api });

      const added = api.addedByTarget.get(10)?.[0] ?? [];
      assert.ok(added.includes("bounty: 💰 $100"));
      assert.ok(added.includes("bounty: 💰 $200"));
    });

    it("skips a linked issue that fails to fetch without aborting", async () => {
      const pr = makePr({ number: 10, body: "Closes #1\nCloses #999" });
      const issue1 = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $300") });
      // issue #999 will throw (not in mock store)
      const api = makeMockApi(pr, [issue1]);
      const patch = await syncPr({ prNumber: 10, api });

      // Issue #1 label still propagated
      assert.ok(api.addedByTarget.get(10)?.[0]?.includes("bounty: 💰 $300"));
    });
  });

  describe("rewarded on merge", () => {
    it("adds rewarded to PR and linked issue on merge", async () => {
      const pr = makePr({
        number: 10,
        merged: true,
        body: "Closes #1",
        labels: labelNames("bounty: 💰 $500"),
      });
      const issue = makeIssue({
        number: 1,
        labels: labelNames("bounty: 💰 $500", BOUNTY_CLAIMED),
      });
      const api = makeMockApi(pr, [issue]);
      await syncPr({ prNumber: 10, api });

      assert.ok(api.addedByTarget.get(10)?.[0]?.includes(BOUNTY_REWARDED));
      assert.ok(api.addedByTarget.get(1)?.[0]?.includes(BOUNTY_REWARDED));
      assert.ok(api.removedByTarget.get(1)?.includes(BOUNTY_CLAIMED));
    });

    it("does not add rewarded again when already present on PR and issue", async () => {
      const pr = makePr({
        number: 10,
        merged: true,
        body: "Closes #1",
        labels: labelNames("bounty: 💰 $500", BOUNTY_REWARDED),
      });
      const issue = makeIssue({
        number: 1,
        labels: labelNames("bounty: 💰 $500", BOUNTY_REWARDED),
      });
      const api = makeMockApi(pr, [issue]);
      const patch = await syncPr({ prNumber: 10, api });
      assert.deepEqual(patch.ops, []);
    });

    it("does not add rewarded when PR was closed but not merged", async () => {
      const pr = makePr({
        number: 10,
        state: "closed",
        merged: false,
        body: "Closes #1",
        labels: labelNames("bounty: 💰 $500"),
      });
      const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $500") });
      const api = makeMockApi(pr, [issue]);
      await syncPr({ prNumber: 10, api });

      assert.ok(!api.addedByTarget.get(10)?.[0]?.includes(BOUNTY_REWARDED));
    });

    it("handles merged PR with no linked issues gracefully", async () => {
      const pr = makePr({
        number: 10,
        merged: true,
        body: "No issue links",
        labels: labelNames("bounty: 💰 $500"),
      });
      const api = makeMockApi(pr, []);
      const patch = await syncPr({ prNumber: 10, api });

      // Only the PR op
      assert.ok(api.addedByTarget.get(10)?.[0]?.includes(BOUNTY_REWARDED));
      assert.equal(patch.ops.length, 1);
    });
  });
});
