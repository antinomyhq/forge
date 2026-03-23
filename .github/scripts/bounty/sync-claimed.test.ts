import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { run } from "./sync-claimed.js";
import type { GitHubApi, Label, Assignee, IssuesEvent } from "./github-api.js";

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

function makeIssuesEvent(overrides: {
  action: string;
  issueLabels?: Label[];
  assignees?: Assignee[];
}): IssuesEvent {
  return {
    action: overrides.action,
    issue: {
      number: 42,
      labels: overrides.issueLabels ?? [],
      assignees: overrides.assignees ?? [],
    },
  };
}

const BOUNTY_LABEL: Label = { name: "bounty: 💰 $500" };
const CLAIMED_LABEL = "bounty: claimed";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("sync-claimed", () => {
  describe("when issue has no bounty label", () => {
    it("does nothing on assigned", async () => {
      const api = makeMockApi();
      const event = makeIssuesEvent({ action: "assigned", issueLabels: [{ name: "bug" }] });
      await run({ issueNumber: 42, event, api });
      assert.equal(api.addedLabels.size, 0);
    });

    it("does nothing on unassigned", async () => {
      const api = makeMockApi();
      const event = makeIssuesEvent({ action: "unassigned", issueLabels: [] });
      await run({ issueNumber: 42, event, api });
      assert.equal(api.removedLabels.size, 0);
    });
  });

  describe("on assigned", () => {
    it("adds bounty: claimed when issue has a bounty label and no claimed label", async () => {
      const api = makeMockApi();
      const event = makeIssuesEvent({
        action: "assigned",
        issueLabels: [BOUNTY_LABEL],
        assignees: [{ login: "alice" }],
      });
      await run({ issueNumber: 42, event, api });
      assert.deepEqual(api.addedLabels.get(42), [CLAIMED_LABEL]);
    });

    it("does not re-add bounty: claimed if already present", async () => {
      const api = makeMockApi();
      const event = makeIssuesEvent({
        action: "assigned",
        issueLabels: [BOUNTY_LABEL, { name: CLAIMED_LABEL }],
        assignees: [{ login: "alice" }],
      });
      await run({ issueNumber: 42, event, api });
      assert.equal(api.addedLabels.size, 0);
    });
  });

  describe("on unassigned", () => {
    it("removes bounty: claimed when last assignee is removed", async () => {
      const api = makeMockApi();
      const event = makeIssuesEvent({
        action: "unassigned",
        issueLabels: [BOUNTY_LABEL, { name: CLAIMED_LABEL }],
        assignees: [], // no one left
      });
      await run({ issueNumber: 42, event, api });
      assert.deepEqual(api.removedLabels.get(42), [CLAIMED_LABEL]);
    });

    it("keeps bounty: claimed when other assignees remain", async () => {
      const api = makeMockApi();
      const event = makeIssuesEvent({
        action: "unassigned",
        issueLabels: [BOUNTY_LABEL, { name: CLAIMED_LABEL }],
        assignees: [{ login: "bob" }], // someone still assigned
      });
      await run({ issueNumber: 42, event, api });
      assert.equal(api.removedLabels.size, 0);
    });

    it("does not attempt removal when claimed label is not present", async () => {
      const api = makeMockApi();
      const event = makeIssuesEvent({
        action: "unassigned",
        issueLabels: [BOUNTY_LABEL],
        assignees: [],
      });
      await run({ issueNumber: 42, event, api });
      assert.equal(api.removedLabels.size, 0);
    });
  });
});
