import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { run } from "./sync-generic-bounty.js";
import type { GitHubApi, Label, IssuesEvent } from "./github-api.js";

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

// IssuesEvent extended with the `label` field present on labeled/unlabeled events.
type LabelEvent = IssuesEvent & { label: Label };

function makeLabelEvent(overrides: {
  action: "labeled" | "unlabeled";
  changedLabel: string;
  issueLabels?: Label[];
  issueNumber?: number;
}): LabelEvent {
  return {
    action: overrides.action,
    label: { name: overrides.changedLabel },
    issue: {
      number: overrides.issueNumber ?? 42,
      // issue.labels reflects the state AFTER the action:
      // - on "labeled":   the new label IS already in the list
      // - on "unlabeled": the removed label is NOT in the list
      labels: overrides.issueLabels ?? [],
      assignees: [],
    },
  };
}

const VALUE_LABEL = "bounty: 💰 $500";
const GENERIC = "bounty";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("sync-generic-bounty", () => {
  describe("non-label actions", () => {
    it("skips assigned action", async () => {
      const api = makeMockApi();
      await run({
        issueNumber: 42,
        event: { action: "assigned", issue: { number: 42, labels: [], assignees: [] } },
        api,
      });
      assert.equal(api.addedLabels.size, 0);
      assert.equal(api.removedLabels.size, 0);
    });
  });

  describe("labeled — non-bounty label", () => {
    it("does nothing when a non-bounty label is added", async () => {
      const api = makeMockApi();
      const event = makeLabelEvent({
        action: "labeled",
        changedLabel: "bug",
        issueLabels: [{ name: "bug" }],
      });
      await run({ issueNumber: 42, event, api });
      assert.equal(api.addedLabels.size, 0);
    });
  });

  describe("labeled — bounty value label", () => {
    it("adds generic bounty label when value label is added and generic is absent", async () => {
      const api = makeMockApi();
      const event = makeLabelEvent({
        action: "labeled",
        changedLabel: VALUE_LABEL,
        // generic label not yet present
        issueLabels: [{ name: VALUE_LABEL }],
      });
      await run({ issueNumber: 42, event, api });
      assert.deepEqual(api.addedLabels.get(42), [GENERIC]);
    });

    it("does not re-add generic label when it is already present", async () => {
      const api = makeMockApi();
      const event = makeLabelEvent({
        action: "labeled",
        changedLabel: VALUE_LABEL,
        issueLabels: [{ name: VALUE_LABEL }, { name: GENERIC }],
      });
      await run({ issueNumber: 42, event, api });
      assert.equal(api.addedLabels.size, 0);
    });

    it("works for all fibonacci value labels", async () => {
      const values = [
        "bounty: 💰 $100",
        "bounty: 💰 $200",
        "bounty: 💰 $300",
        "bounty: 💰 $500",
        "bounty: 💰 $800",
        "bounty: 💰 $1300",
        "bounty: 💰 $2100",
        "bounty: 💰 $3400",
        "bounty: 💰 $5500",
      ];
      for (const label of values) {
        const api = makeMockApi();
        const event = makeLabelEvent({
          action: "labeled",
          changedLabel: label,
          issueLabels: [{ name: label }],
        });
        await run({ issueNumber: 42, event, api });
        assert.deepEqual(
          api.addedLabels.get(42),
          [GENERIC],
          `should add generic bounty for label "${label}"`
        );
      }
    });
  });

  describe("unlabeled — non-bounty label", () => {
    it("does nothing when a non-bounty label is removed", async () => {
      const api = makeMockApi();
      const event = makeLabelEvent({
        action: "unlabeled",
        changedLabel: "bug",
        issueLabels: [{ name: VALUE_LABEL }, { name: GENERIC }],
      });
      await run({ issueNumber: 42, event, api });
      assert.equal(api.removedLabels.size, 0);
    });
  });

  describe("unlabeled — bounty value label", () => {
    it("removes generic label when last value label is removed", async () => {
      const api = makeMockApi();
      const event = makeLabelEvent({
        action: "unlabeled",
        changedLabel: VALUE_LABEL,
        // after removal: no value labels remain, but generic is still present
        issueLabels: [{ name: GENERIC }],
      });
      await run({ issueNumber: 42, event, api });
      assert.deepEqual(api.removedLabels.get(42), [GENERIC]);
    });

    it("keeps generic label when other value labels remain", async () => {
      const api = makeMockApi();
      const event = makeLabelEvent({
        action: "unlabeled",
        changedLabel: VALUE_LABEL,
        // another value label still present after removal
        issueLabels: [{ name: "bounty: 💰 $300" }, { name: GENERIC }],
      });
      await run({ issueNumber: 42, event, api });
      assert.equal(api.removedLabels.size, 0);
    });

    it("does nothing when generic label is already absent", async () => {
      const api = makeMockApi();
      const event = makeLabelEvent({
        action: "unlabeled",
        changedLabel: VALUE_LABEL,
        // generic label never existed
        issueLabels: [],
      });
      await run({ issueNumber: 42, event, api });
      assert.equal(api.removedLabels.size, 0);
    });
  });
});
