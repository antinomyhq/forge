import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { run } from "./mark-rewarded.js";
import type { GitHubApi, Label, PullRequestEvent } from "./github-api.js";

// ---------------------------------------------------------------------------
// Mock factory
// ---------------------------------------------------------------------------

function makeMockApi(overrides: Partial<GitHubApi> = {}): GitHubApi & {
  addedLabels: Map<number, string[]>;
  removedLabels: Map<number, string[]>;
} {
  const addedLabels = new Map<number, string[]>();
  const removedLabels = new Map<number, string[]>();

  return {
    addedLabels,
    removedLabels,
    getLabels: async (_n) => [],
    addLabels: async (n, labels) => {
      addedLabels.set(n, [...(addedLabels.get(n) ?? []), ...labels]);
    },
    removeLabel: async (n, label) => {
      removedLabels.set(n, [...(removedLabels.get(n) ?? []), label]);
    },
    addComment: async (_n, _b) => {},
    ...overrides,
  };
}

function makePrEvent(overrides: {
  merged?: boolean;
  body?: string;
  prLabels?: Label[];
  prNumber?: number;
}): PullRequestEvent {
  return {
    pull_request: {
      number: overrides.prNumber ?? 10,
      merged: overrides.merged ?? true,
      body: overrides.body ?? "",
      html_url: "https://github.com/owner/repo/pull/10",
      labels: overrides.prLabels ?? [],
      user: { login: "alice" },
    },
  };
}

const BOUNTY_LABEL: Label = { name: "bounty: 💰 $500" };
const REWARDED_LABEL = "bounty: rewarded";
const CLAIMED_LABEL = "bounty: claimed";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("mark-rewarded", () => {
  describe("when PR was not merged", () => {
    it("does nothing", async () => {
      const api = makeMockApi();
      const event = makePrEvent({ merged: false, prLabels: [BOUNTY_LABEL] });
      await run({ prNumber: 10, event, api });
      assert.equal(api.addedLabels.size, 0);
      assert.equal(api.removedLabels.size, 0);
    });
  });

  describe("when merged PR has no bounty label", () => {
    it("does nothing", async () => {
      const api = makeMockApi();
      const event = makePrEvent({ merged: true, prLabels: [{ name: "bug" }] });
      await run({ prNumber: 10, event, api });
      assert.equal(api.addedLabels.size, 0);
    });
  });

  describe("when merged PR has a bounty label", () => {
    it("adds bounty: rewarded to the PR", async () => {
      const api = makeMockApi();
      const event = makePrEvent({ merged: true, prLabels: [BOUNTY_LABEL] });
      await run({ prNumber: 10, event, api });
      assert.ok(api.addedLabels.get(10)?.includes(REWARDED_LABEL));
    });

    it("does not re-add bounty: rewarded if PR already has it", async () => {
      const api = makeMockApi();
      const event = makePrEvent({
        merged: true,
        prLabels: [BOUNTY_LABEL, { name: REWARDED_LABEL }],
      });
      await run({ prNumber: 10, event, api });
      assert.equal(api.addedLabels.get(10), undefined);
    });

    it("adds bounty: rewarded to the linked issue", async () => {
      const api = makeMockApi({
        getLabels: async (n) => (n === 42 ? [] : []),
      });
      const event = makePrEvent({
        merged: true,
        prLabels: [BOUNTY_LABEL],
        body: "Closes #42",
      });
      await run({ prNumber: 10, event, api });
      assert.ok(api.addedLabels.get(42)?.includes(REWARDED_LABEL));
    });

    it("removes bounty: claimed from the linked issue", async () => {
      const api = makeMockApi({
        getLabels: async (n) =>
          n === 42 ? [{ name: CLAIMED_LABEL }] : [],
      });
      const event = makePrEvent({
        merged: true,
        prLabels: [BOUNTY_LABEL],
        body: "Fixes #42",
      });
      await run({ prNumber: 10, event, api });
      assert.ok(api.removedLabels.get(42)?.includes(CLAIMED_LABEL));
    });

    it("does not attempt to remove claimed if issue does not have it", async () => {
      const api = makeMockApi({
        getLabels: async (_n) => [],
      });
      const event = makePrEvent({
        merged: true,
        prLabels: [BOUNTY_LABEL],
        body: "Fixes #42",
      });
      await run({ prNumber: 10, event, api });
      assert.equal(api.removedLabels.size, 0);
    });

    it("processes multiple linked issues from closing keywords", async () => {
      const api = makeMockApi({
        getLabels: async (n) =>
          n === 1 ? [{ name: CLAIMED_LABEL }] : [{ name: CLAIMED_LABEL }],
      });
      const event = makePrEvent({
        merged: true,
        prLabels: [BOUNTY_LABEL],
        body: "Closes #1\nResolves #2",
      });
      await run({ prNumber: 10, event, api });
      assert.ok(api.addedLabels.get(1)?.includes(REWARDED_LABEL));
      assert.ok(api.addedLabels.get(2)?.includes(REWARDED_LABEL));
      assert.ok(api.removedLabels.get(1)?.includes(CLAIMED_LABEL));
      assert.ok(api.removedLabels.get(2)?.includes(CLAIMED_LABEL));
    });

    it("continues processing remaining issues when one getLabels call throws", async () => {
      const api = makeMockApi({
        getLabels: async (n) => {
          if (n === 1) throw new Error("network error");
          return [{ name: CLAIMED_LABEL }];
        },
      });
      const event = makePrEvent({
        merged: true,
        prLabels: [BOUNTY_LABEL],
        body: "Closes #1\nCloses #2",
      });
      await run({ prNumber: 10, event, api });
      assert.ok(api.addedLabels.get(2)?.includes(REWARDED_LABEL));
      assert.ok(api.removedLabels.get(2)?.includes(CLAIMED_LABEL));
    });
  });
});
