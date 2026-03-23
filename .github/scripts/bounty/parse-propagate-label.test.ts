import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { parse } from "./parse-propagate-label.js";
import type { PullRequestEvent } from "./github-api.js";

function makePrEvent(overrides: {
  body?: string;
  prNumber?: number;
  prLabels?: string[];
  author?: string;
  htmlUrl?: string;
}): PullRequestEvent {
  return {
    pull_request: {
      number: overrides.prNumber ?? 10,
      merged: false,
      body: overrides.body ?? "",
      html_url: overrides.htmlUrl ?? "https://github.com/owner/repo/pull/10",
      labels: (overrides.prLabels ?? []).map((name) => ({ name })),
      user: { login: overrides.author ?? "alice" },
    },
  };
}

describe("parse-propagate-label", () => {
  it("returns empty intent when PR body has no closing keywords", () => {
    const result = parse({ prNumber: 10, event: makePrEvent({ body: "Just a description" }) });
    assert.deepEqual(result.mutations, []);
    assert.deepEqual(result.knownLabels, {});
    assert.equal(result.labelCopies, undefined);
  });

  it("emits labelCopies with linked issue numbers", () => {
    const result = parse({ prNumber: 10, event: makePrEvent({ body: "Closes #42" }) });
    assert.deepEqual(result.labelCopies, { sources: [42], prTarget: 10 });
  });

  it("emits a comment mutation for each linked issue", () => {
    const result = parse({ prNumber: 10, event: makePrEvent({ body: "Fixes #42", author: "bob", htmlUrl: "https://github.com/owner/repo/pull/10" }) });
    assert.equal(result.mutations.length, 1);
    assert.equal(result.mutations[0]!.target, 42);
    assert.ok(result.mutations[0]!.comment?.includes("@bob"));
    assert.ok(result.mutations[0]!.comment?.includes("https://github.com/owner/repo/pull/10"));
  });

  it("emits comment mutations for multiple linked issues", () => {
    const result = parse({ prNumber: 10, event: makePrEvent({ body: "Closes #1\nResolves #2" }) });
    const targets = result.mutations.map((m) => m.target);
    assert.deepEqual(targets, [1, 2]);
    assert.deepEqual(result.labelCopies?.sources, [1, 2]);
  });

  it("recognises all closing keyword variants", () => {
    for (const kw of ["closes", "Closes", "fixes", "Fixes", "resolves", "Resolves"]) {
      const result = parse({ prNumber: 10, event: makePrEvent({ body: `${kw} #99` }) });
      assert.deepEqual(result.labelCopies?.sources, [99], `keyword "${kw}" should be recognised`);
    }
  });

  it("puts current PR labels in knownLabels to enable dedup in plan step", () => {
    const result = parse({
      prNumber: 10,
      event: makePrEvent({ body: "Closes #42", prLabels: ["bounty: $500"] }),
    });
    assert.deepEqual(result.knownLabels[10], ["bounty: $500"]);
  });

  it("emits empty add and remove on comment mutations", () => {
    const result = parse({ prNumber: 10, event: makePrEvent({ body: "Closes #42" }) });
    assert.deepEqual(result.mutations[0]!.add, []);
    assert.deepEqual(result.mutations[0]!.remove, []);
  });
});
