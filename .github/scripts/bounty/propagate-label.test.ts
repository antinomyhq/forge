import { describe, it, mock, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { run } from "./propagate-label.js";
import type { GitHubApi, Label, PullRequestEvent } from "./github-api.js";

// ---------------------------------------------------------------------------
// Mock factory
// ---------------------------------------------------------------------------

function makeMockApi(overrides: Partial<GitHubApi> = {}): GitHubApi & {
  addedLabels: Map<number, string[]>;
  addedComments: Map<number, string[]>;
} {
  const addedLabels = new Map<number, string[]>();
  const addedComments = new Map<number, string[]>();

  return {
    addedLabels,
    addedComments,
    getLabels: async (_n) => [],
    addLabels: async (n, labels) => {
      addedLabels.set(n, [...(addedLabels.get(n) ?? []), ...labels]);
    },
    removeLabel: async (_n, _l) => {},
    addComment: async (n, body) => {
      addedComments.set(n, [...(addedComments.get(n) ?? []), body]);
    },
    ...overrides,
  };
}

function makePrEvent(overrides: {
  body?: string;
  prLabels?: Label[];
  prNumber?: number;
  author?: string;
  htmlUrl?: string;
}): PullRequestEvent {
  return {
    pull_request: {
      number: overrides.prNumber ?? 10,
      merged: false,
      body: overrides.body ?? "",
      html_url: overrides.htmlUrl ?? "https://github.com/owner/repo/pull/10",
      labels: overrides.prLabels ?? [],
      user: { login: overrides.author ?? "alice" },
    },
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("propagate-label", () => {
  describe("when PR body has no closing keywords", () => {
    it("does nothing and logs a message", async () => {
      const api = makeMockApi();
      await run({ prNumber: 10, event: makePrEvent({ body: "Just a description" }), api });
      assert.equal(api.addedLabels.size, 0);
      assert.equal(api.addedComments.size, 0);
    });
  });

  describe("when linked issue has no bounty label", () => {
    it("does not add any label to the PR or post a comment", async () => {
      const api = makeMockApi({
        getLabels: async (n) => (n === 42 ? [{ name: "bug" }] : []),
      });
      const event = makePrEvent({ body: "Fixes #42" });
      await run({ prNumber: 10, event, api });
      assert.equal(api.addedLabels.size, 0);
      assert.equal(api.addedComments.size, 0);
    });
  });

  describe("when linked issue has a bounty label", () => {
    it("copies the bounty label to the PR", async () => {
      const api = makeMockApi({
        getLabels: async (n) =>
          n === 42 ? [{ name: "bounty: 💰 $500" }] : [],
      });
      const event = makePrEvent({ body: "Closes #42" });
      await run({ prNumber: 10, event, api });
      assert.deepEqual(api.addedLabels.get(10), ["bounty: 💰 $500"]);
    });

    it("posts a notification comment on the linked issue", async () => {
      const api = makeMockApi({
        getLabels: async (n) =>
          n === 42 ? [{ name: "bounty: 💰 $500" }] : [],
      });
      const event = makePrEvent({
        body: "Closes #42",
        author: "bob",
        htmlUrl: "https://github.com/owner/repo/pull/10",
      });
      await run({ prNumber: 10, event, api });
      const comments = api.addedComments.get(42) ?? [];
      assert.equal(comments.length, 1);
      assert.ok(comments[0]!.includes("@bob"));
      assert.ok(comments[0]!.includes("https://github.com/owner/repo/pull/10"));
    });

    it("does not re-add a bounty label the PR already has", async () => {
      const api = makeMockApi({
        getLabels: async (n) =>
          n === 42
            ? [{ name: "bounty: 💰 $500" }]
            : [{ name: "bounty: 💰 $500" }], // PR already has it
      });
      const event = makePrEvent({ body: "Closes #42" });
      await run({ prNumber: 10, event, api });
      assert.equal(api.addedLabels.get(10), undefined);
    });

    it("handles multiple closing keywords in the same body", async () => {
      const api = makeMockApi({
        getLabels: async (n) => {
          if (n === 1) return [{ name: "bounty: 💰 $100" }];
          if (n === 2) return [{ name: "bounty: 💰 $200" }];
          return [];
        },
      });
      const event = makePrEvent({ body: "Fixes #1\nResolves #2" });
      await run({ prNumber: 10, event, api });
      assert.deepEqual(api.addedLabels.get(10), ["bounty: 💰 $100", "bounty: 💰 $200"]);
      assert.ok(api.addedComments.has(1));
      assert.ok(api.addedComments.has(2));
    });

    it("handles all closing keyword variants: closes, fixes, resolves", async () => {
      for (const keyword of ["Closes", "closes", "Fixes", "fixes", "Resolves", "resolves"]) {
        const api = makeMockApi({
          getLabels: async (n) =>
            n === 99 ? [{ name: "bounty: 💰 $300" }] : [],
        });
        const event = makePrEvent({ body: `${keyword} #99` });
        await run({ prNumber: 10, event, api });
        assert.deepEqual(
          api.addedLabels.get(10),
          ["bounty: 💰 $300"],
          `keyword "${keyword}" should be recognised`
        );
      }
    });

    it("continues processing remaining issues when one getLabels call throws", async () => {
      const api = makeMockApi({
        getLabels: async (n) => {
          if (n === 1) throw new Error("network error");
          if (n === 10) return []; // PR has no labels yet
          return [{ name: "bounty: 💰 $200" }];
        },
      });
      const event = makePrEvent({ body: "Fixes #1\nFixes #2" });
      await run({ prNumber: 10, event, api });
      assert.deepEqual(api.addedLabels.get(10), ["bounty: 💰 $200"]);
    });
  });
});
