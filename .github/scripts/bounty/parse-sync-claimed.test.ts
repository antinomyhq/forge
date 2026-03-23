import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { parse } from "./parse-sync-claimed.js";
import type { IssuesEvent } from "./github-api.js";

function makeEvent(overrides: {
  action: string;
  issueLabels?: string[];
  assignees?: string[];
}): IssuesEvent {
  return {
    action: overrides.action,
    issue: {
      number: 42,
      labels: (overrides.issueLabels ?? []).map((name) => ({ name })),
      assignees: (overrides.assignees ?? []).map((login) => ({ login })),
    },
  };
}

describe("parse-sync-claimed", () => {
  describe("issue has no bounty value label", () => {
    it("returns empty intent on assigned", () => {
      const result = parse({ issueNumber: 42, event: makeEvent({ action: "assigned", issueLabels: ["bug"] }) });
      assert.deepEqual(result.mutations, []);
    });

    it("returns empty intent on unassigned", () => {
      const result = parse({ issueNumber: 42, event: makeEvent({ action: "unassigned" }) });
      assert.deepEqual(result.mutations, []);
    });
  });

  describe("assigned with a bounty value label", () => {
    it("emits add bounty: claimed mutation", () => {
      const result = parse({
        issueNumber: 42,
        event: makeEvent({ action: "assigned", issueLabels: ["bounty: $500"], assignees: ["alice"] }),
      });
      assert.equal(result.mutations.length, 1);
      assert.deepEqual(result.mutations[0], { target: 42, add: ["bounty: claimed"], remove: [] });
    });

    it("provides current labels in knownLabels for plan dedup", () => {
      const result = parse({
        issueNumber: 42,
        event: makeEvent({ action: "assigned", issueLabels: ["bounty: $500", "bounty: claimed"] }),
      });
      assert.deepEqual(result.knownLabels[42], ["bounty: $500", "bounty: claimed"]);
    });
  });

  describe("unassigned with a bounty value label", () => {
    it("emits remove bounty: claimed when last assignee gone", () => {
      const result = parse({
        issueNumber: 42,
        event: makeEvent({ action: "unassigned", issueLabels: ["bounty: $500", "bounty: claimed"], assignees: [] }),
      });
      assert.deepEqual(result.mutations[0], { target: 42, add: [], remove: ["bounty: claimed"] });
    });

    it("returns empty intent when other assignees remain", () => {
      const result = parse({
        issueNumber: 42,
        event: makeEvent({ action: "unassigned", issueLabels: ["bounty: $500", "bounty: claimed"], assignees: ["bob"] }),
      });
      assert.deepEqual(result.mutations, []);
    });
  });
});
