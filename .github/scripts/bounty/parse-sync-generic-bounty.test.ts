import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { parse } from "./parse-sync-generic-bounty.js";
import type { IssuesEvent } from "./github-api.js";

function makeEvent(overrides: {
  action: string;
  changedLabel?: string;
  issueLabels?: string[];
}): IssuesEvent {
  return {
    action: overrides.action,
    label: overrides.changedLabel ? { name: overrides.changedLabel } : undefined,
    issue: {
      number: 42,
      labels: (overrides.issueLabels ?? []).map((name) => ({ name })),
      assignees: [],
    },
  };
}

describe("parse-sync-generic-bounty", () => {
  it("returns empty intent for non-label actions", () => {
    const result = parse({ issueNumber: 42, event: makeEvent({ action: "assigned" }) });
    assert.deepEqual(result.mutations, []);
  });

  it("returns empty intent when changed label is not a bounty value label", () => {
    const result = parse({ issueNumber: 42, event: makeEvent({ action: "labeled", changedLabel: "bug", issueLabels: ["bug"] }) });
    assert.deepEqual(result.mutations, []);
  });

  describe("labeled", () => {
    it("emits add bounty mutation when value label is applied", () => {
      const result = parse({
        issueNumber: 42,
        event: makeEvent({ action: "labeled", changedLabel: "bounty: $500", issueLabels: ["bounty: $500"] }),
      });
      assert.deepEqual(result.mutations[0], { target: 42, add: ["bounty"], remove: [] });
    });

    it("provides current labels in knownLabels", () => {
      const result = parse({
        issueNumber: 42,
        event: makeEvent({ action: "labeled", changedLabel: "bounty: $500", issueLabels: ["bounty: $500", "bounty"] }),
      });
      assert.deepEqual(result.knownLabels[42], ["bounty: $500", "bounty"]);
    });
  });

  describe("unlabeled", () => {
    it("emits remove bounty mutation when last value label is removed", () => {
      const result = parse({
        issueNumber: 42,
        // After removal: no value labels remain, generic label still present
        event: makeEvent({ action: "unlabeled", changedLabel: "bounty: $500", issueLabels: ["bounty"] }),
      });
      assert.deepEqual(result.mutations[0], { target: 42, add: [], remove: ["bounty"] });
    });

    it("returns empty intent when other value labels remain", () => {
      const result = parse({
        issueNumber: 42,
        event: makeEvent({ action: "unlabeled", changedLabel: "bounty: $500", issueLabels: ["bounty: $300", "bounty"] }),
      });
      assert.deepEqual(result.mutations, []);
    });

    it("returns empty intent when no changed label is present", () => {
      const result = parse({
        issueNumber: 42,
        event: makeEvent({ action: "unlabeled", issueLabels: ["bounty"] }),
      });
      assert.deepEqual(result.mutations, []);
    });
  });
});
