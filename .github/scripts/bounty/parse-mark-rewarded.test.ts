import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { parse } from "./parse-mark-rewarded.js";
import type { PullRequestEvent } from "./github-api.js";

function makePrEvent(overrides: {
  merged?: boolean;
  body?: string;
  prLabels?: string[];
  prNumber?: number;
}): PullRequestEvent {
  return {
    pull_request: {
      number: overrides.prNumber ?? 10,
      merged: overrides.merged ?? true,
      body: overrides.body ?? "",
      html_url: "https://github.com/owner/repo/pull/10",
      labels: (overrides.prLabels ?? []).map((name) => ({ name })),
      user: { login: "alice" },
    },
  };
}

describe("parse-mark-rewarded", () => {
  it("returns empty intent when PR was not merged", () => {
    const result = parse({ prNumber: 10, event: makePrEvent({ merged: false, prLabels: ["bounty: $500"] }) });
    assert.deepEqual(result.mutations, []);
  });

  it("returns empty intent when merged PR has no bounty value label", () => {
    const result = parse({ prNumber: 10, event: makePrEvent({ merged: true, prLabels: ["bug"] }) });
    assert.deepEqual(result.mutations, []);
  });

  it("emits add bounty: rewarded mutation for the PR", () => {
    const result = parse({ prNumber: 10, event: makePrEvent({ merged: true, prLabels: ["bounty: $500"] }) });
    assert.ok(result.mutations.some((m) => m.target === 10 && m.add.includes("bounty: rewarded")));
  });

  it("puts PR labels in knownLabels", () => {
    const result = parse({ prNumber: 10, event: makePrEvent({ merged: true, prLabels: ["bounty: $500"] }) });
    assert.deepEqual(result.knownLabels[10], ["bounty: $500"]);
  });

  it("emits add rewarded + remove claimed mutations for linked issues", () => {
    const result = parse({
      prNumber: 10,
      event: makePrEvent({ merged: true, prLabels: ["bounty: $500"], body: "Closes #42" }),
    });
    const issueMutation = result.mutations.find((m) => m.target === 42);
    assert.ok(issueMutation);
    assert.deepEqual(issueMutation.add, ["bounty: rewarded"]);
    assert.deepEqual(issueMutation.remove, ["bounty: claimed"]);
  });

  it("does not include issue labels in knownLabels (they are fetched by plan)", () => {
    const result = parse({
      prNumber: 10,
      event: makePrEvent({ merged: true, prLabels: ["bounty: $500"], body: "Closes #42" }),
    });
    assert.equal(result.knownLabels[42], undefined);
  });

  it("handles multiple linked issues", () => {
    const result = parse({
      prNumber: 10,
      event: makePrEvent({ merged: true, prLabels: ["bounty: $500"], body: "Closes #1\nFixes #2" }),
    });
    const targets = result.mutations.map((m) => m.target);
    assert.ok(targets.includes(1));
    assert.ok(targets.includes(2));
  });
});
