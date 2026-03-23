import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { plan } from "./plan.js";
import { execute } from "./execute.js";
import type { GitHubApi, Label, ParsedIntent, BatchPlan } from "./github-api.js";

// ---------------------------------------------------------------------------
// Mock API factory
// ---------------------------------------------------------------------------

function makeMockApi(overrides: Partial<GitHubApi> = {}): GitHubApi & {
  addedLabels: Map<number, string[]>;
  removedLabels: Map<number, string[]>;
  addedComments: Map<number, string[]>;
  fetchedTargets: number[];
} {
  const addedLabels = new Map<number, string[]>();
  const removedLabels = new Map<number, string[]>();
  const addedComments = new Map<number, string[]>();
  const fetchedTargets: number[] = [];

  return {
    addedLabels,
    removedLabels,
    addedComments,
    fetchedTargets,
    getLabels: async (n) => {
      fetchedTargets.push(n);
      return [];
    },
    addLabels: async (n, labels) => {
      addedLabels.set(n, [...(addedLabels.get(n) ?? []), ...labels]);
    },
    removeLabel: async (n, label) => {
      removedLabels.set(n, [...(removedLabels.get(n) ?? []), label]);
    },
    addComment: async (n, body) => {
      addedComments.set(n, [...(addedComments.get(n) ?? []), body]);
    },
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// plan() tests
// ---------------------------------------------------------------------------

describe("plan", () => {
  describe("knownLabels deduplication", () => {
    it("skips API fetch when labels are in knownLabels", async () => {
      const api = makeMockApi();
      const intent: ParsedIntent = {
        mutations: [{ target: 42, add: ["bounty: claimed"], remove: [] }],
        knownLabels: { 42: [] },
      };
      await plan({ intent, api });
      assert.deepEqual(api.fetchedTargets, []);
    });

    it("filters out add labels already present in knownLabels", async () => {
      const api = makeMockApi();
      const intent: ParsedIntent = {
        mutations: [{ target: 42, add: ["bounty: claimed"], remove: [] }],
        knownLabels: { 42: ["bounty: claimed"] },
      };
      const result = await plan({ intent, api });
      assert.deepEqual(result.mutations, []);
    });

    it("filters out remove labels not present in knownLabels", async () => {
      const api = makeMockApi();
      const intent: ParsedIntent = {
        mutations: [{ target: 42, add: [], remove: ["bounty: claimed"] }],
        knownLabels: { 42: [] },
      };
      const result = await plan({ intent, api });
      assert.deepEqual(result.mutations, []);
    });
  });

  describe("label fetching", () => {
    it("fetches labels when target not in knownLabels", async () => {
      const api = makeMockApi({
        getLabels: async (n) => {
          if (n === 42) return [{ name: "bounty: $500" }];
          return [];
        },
      });
      const intent: ParsedIntent = {
        mutations: [{ target: 42, add: ["bounty: claimed"], remove: [] }],
        knownLabels: {},
      };
      const result = await plan({ intent, api });
      assert.equal(result.mutations.length, 1);
      assert.deepEqual(result.mutations[0]!.add, ["bounty: claimed"]);
    });

    it("skips target when getLabels throws", async () => {
      const api = makeMockApi({
        getLabels: async () => { throw new Error("network error"); },
      });
      const intent: ParsedIntent = {
        mutations: [{ target: 42, add: ["bounty: claimed"], remove: [] }],
        knownLabels: {},
      };
      const result = await plan({ intent, api });
      assert.deepEqual(result.mutations, []);
    });
  });

  describe("labelCopies", () => {
    it("fetches source issue labels and merges bounty labels into PR mutation", async () => {
      const api = makeMockApi({
        getLabels: async (n) => {
          if (n === 42) return [{ name: "bounty: $500" }, { name: "bug" }];
          return [];
        },
      });
      const intent: ParsedIntent = {
        mutations: [{ target: 42, add: [], remove: [], comment: "PR opened" }],
        knownLabels: { 10: [] }, // PR has no labels yet
        labelCopies: { sources: [42], prTarget: 10 },
      };
      const result = await plan({ intent, api });

      const prMutation = result.mutations.find((m) => m.target === 10);
      assert.ok(prMutation, "PR mutation should be present");
      assert.deepEqual(prMutation.add, ["bounty: $500"]);
    });

    it("does not add non-bounty labels from source issue to PR", async () => {
      const api = makeMockApi({
        getLabels: async (n) => {
          if (n === 42) return [{ name: "bug" }, { name: "enhancement" }];
          return [];
        },
      });
      const intent: ParsedIntent = {
        mutations: [{ target: 42, add: [], remove: [], comment: "PR opened" }],
        knownLabels: { 10: [] },
        labelCopies: { sources: [42], prTarget: 10 },
      };
      const result = await plan({ intent, api });
      const prMutation = result.mutations.find((m) => m.target === 10);
      assert.equal(prMutation, undefined);
    });

    it("drops issue comment when source issue has no bounty labels", async () => {
      const api = makeMockApi({
        getLabels: async () => [{ name: "bug" }],
      });
      const intent: ParsedIntent = {
        mutations: [{ target: 42, add: [], remove: [], comment: "PR opened" }],
        knownLabels: { 10: [] },
        labelCopies: { sources: [42], prTarget: 10 },
      };
      const result = await plan({ intent, api });
      const issueMutation = result.mutations.find((m) => m.target === 42);
      assert.equal(issueMutation, undefined);
    });

    it("deduplicates PR labels already in knownLabels[prTarget]", async () => {
      const api = makeMockApi({
        getLabels: async (n) => {
          if (n === 42) return [{ name: "bounty: $500" }];
          return [];
        },
      });
      const intent: ParsedIntent = {
        mutations: [],
        knownLabels: { 10: ["bounty: $500"] }, // PR already has the label
        labelCopies: { sources: [42], prTarget: 10 },
      };
      const result = await plan({ intent, api });
      const prMutation = result.mutations.find((m) => m.target === 10);
      assert.equal(prMutation, undefined);
    });

    it("continues processing other sources when one getLabels throws", async () => {
      const api = makeMockApi({
        getLabels: async (n) => {
          if (n === 1) throw new Error("network error");
          if (n === 2) return [{ name: "bounty: $300" }];
          return [];
        },
      });
      const intent: ParsedIntent = {
        mutations: [
          { target: 1, add: [], remove: [], comment: "PR opened" },
          { target: 2, add: [], remove: [], comment: "PR opened" },
        ],
        knownLabels: { 10: [] },
        labelCopies: { sources: [1, 2], prTarget: 10 },
      };
      const result = await plan({ intent, api });
      const prMutation = result.mutations.find((m) => m.target === 10);
      assert.deepEqual(prMutation?.add, ["bounty: $300"]);
    });

    it("batches bounty labels from multiple source issues into one PR mutation", async () => {
      const api = makeMockApi({
        getLabels: async (n) => {
          if (n === 1) return [{ name: "bounty: $100" }];
          if (n === 2) return [{ name: "bounty: $200" }];
          return [];
        },
      });
      const intent: ParsedIntent = {
        mutations: [
          { target: 1, add: [], remove: [], comment: "PR opened" },
          { target: 2, add: [], remove: [], comment: "PR opened" },
        ],
        knownLabels: { 10: [] },
        labelCopies: { sources: [1, 2], prTarget: 10 },
      };
      const result = await plan({ intent, api });
      const prMutation = result.mutations.find((m) => m.target === 10);
      assert.deepEqual(prMutation?.add.sort(), ["bounty: $100", "bounty: $200"]);
    });
  });
});

// ---------------------------------------------------------------------------
// execute() tests
// ---------------------------------------------------------------------------

describe("execute", () => {
  it("calls addLabels once per target with all labels batched", async () => {
    const api = makeMockApi();
    const batchPlan: BatchPlan = {
      mutations: [{ target: 42, add: ["bounty: claimed", "bounty"], remove: [] }],
    };
    await execute({ plan: batchPlan, api });
    assert.deepEqual(api.addedLabels.get(42), ["bounty: claimed", "bounty"]);
  });

  it("calls removeLabel once per label", async () => {
    const api = makeMockApi();
    const batchPlan: BatchPlan = {
      mutations: [{ target: 42, add: [], remove: ["bounty: claimed", "bounty"] }],
    };
    await execute({ plan: batchPlan, api });
    assert.deepEqual(api.removedLabels.get(42), ["bounty: claimed", "bounty"]);
  });

  it("posts comments", async () => {
    const api = makeMockApi();
    const batchPlan: BatchPlan = {
      mutations: [{ target: 42, add: [], remove: [], comment: "hello" }],
    };
    await execute({ plan: batchPlan, api });
    assert.deepEqual(api.addedComments.get(42), ["hello"]);
  });

  it("handles multiple targets independently", async () => {
    const api = makeMockApi();
    const batchPlan: BatchPlan = {
      mutations: [
        { target: 10, add: ["bounty: rewarded"], remove: [] },
        { target: 42, add: ["bounty: rewarded"], remove: ["bounty: claimed"] },
      ],
    };
    await execute({ plan: batchPlan, api });
    assert.deepEqual(api.addedLabels.get(10), ["bounty: rewarded"]);
    assert.deepEqual(api.addedLabels.get(42), ["bounty: rewarded"]);
    assert.deepEqual(api.removedLabels.get(42), ["bounty: claimed"]);
  });

  it("does not call addLabels when add list is empty", async () => {
    const api = makeMockApi();
    const batchPlan: BatchPlan = {
      mutations: [{ target: 42, add: [], remove: ["bounty: claimed"] }],
    };
    await execute({ plan: batchPlan, api });
    assert.equal(api.addedLabels.size, 0);
  });
});
