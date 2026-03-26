import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  computeIssuePatch,
  computePrPatch,
  linkedIssueNumbers,
  BOUNTY_GENERIC,
  BOUNTY_CLAIMED,
  BOUNTY_REWARDED,
} from "../src/rules.js";
import type { Issue, PullRequest, IssueState, PrState } from "../src/types.js";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function makeIssue(overrides: Partial<Issue> & { number: number }): Issue {
  return {
    title: "Test issue",
    html_url: `https://github.com/owner/repo/issues/${overrides.number}`,
    state: "open",
    labels: [],
    assignees: [],
    ...overrides,
  };
}

function makePr(overrides: Partial<PullRequest> & { number: number }): PullRequest {
  return {
    title: "Test PR",
    state: "open",
    merged: false,
    body: null,
    labels: [],
    user: { login: "contributor" },
    html_url: `https://github.com/owner/repo/pull/${overrides.number}`,
    ...overrides,
  };
}

function labelNames(...names: string[]): { name: string }[] {
  return names.map((name) => ({ name }));
}

function issueState(issue: Issue): IssueState {
  return { issue, currentLabels: new Set(issue.labels.map((l) => l.name)) };
}

// ---------------------------------------------------------------------------
// computeIssuePatch — issue rules
// ---------------------------------------------------------------------------

describe("computeIssuePatch", () => {
  describe("no bounty label", () => {
    it("produces no patch when issue has no bounty labels at all", () => {
      const issue = makeIssue({ number: 1, labels: labelNames("type: bug") });
      const patch = computeIssuePatch(issueState(issue));
      assert.deepEqual(patch.ops, []);
    });

    it("removes generic bounty label when no value label present", () => {
      const issue = makeIssue({ number: 1, labels: labelNames(BOUNTY_GENERIC) });
      const patch = computeIssuePatch(issueState(issue));
      assert.equal(patch.ops.length, 1);
      assert.deepEqual(patch.ops[0]!.remove, [BOUNTY_GENERIC]);
      assert.deepEqual(patch.ops[0]!.add, []);
    });

    it("removes claimed label when no value label present", () => {
      const issue = makeIssue({
        number: 1,
        labels: labelNames(BOUNTY_CLAIMED),
        assignees: [{ login: "alice" }],
      });
      const patch = computeIssuePatch(issueState(issue));
      assert.equal(patch.ops.length, 1);
      assert.deepEqual(patch.ops[0]!.remove, [BOUNTY_CLAIMED]);
    });
  });

  describe("has value label, no assignees", () => {
    it("adds generic label when missing", () => {
      const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $500") });
      const patch = computeIssuePatch(issueState(issue));
      assert.equal(patch.ops.length, 1);
      assert.deepEqual(patch.ops[0]!.add, [BOUNTY_GENERIC]);
      assert.deepEqual(patch.ops[0]!.remove, []);
    });

    it("recognises legacy label format (no emoji) as a value label", () => {
      const issue = makeIssue({ number: 1, labels: labelNames("bounty: $300") });
      const patch = computeIssuePatch(issueState(issue));
      assert.equal(patch.ops.length, 1);
      assert.deepEqual(patch.ops[0]!.add, [BOUNTY_GENERIC]);
      assert.deepEqual(patch.ops[0]!.remove, []);
    });

    it("produces no patch when generic already present and no assignees", () => {
      const issue = makeIssue({
        number: 1,
        labels: labelNames("bounty: 💰 $500", BOUNTY_GENERIC),
      });
      const patch = computeIssuePatch(issueState(issue));
      assert.deepEqual(patch.ops, []);
    });

    it("removes claimed when no assignees", () => {
      const issue = makeIssue({
        number: 1,
        labels: labelNames("bounty: 💰 $500", BOUNTY_GENERIC, BOUNTY_CLAIMED),
        assignees: [],
      });
      const patch = computeIssuePatch(issueState(issue));
      assert.equal(patch.ops.length, 1);
      assert.deepEqual(patch.ops[0]!.remove, [BOUNTY_CLAIMED]);
      assert.deepEqual(patch.ops[0]!.add, []);
    });
  });

  describe("has value label and assignees", () => {
    it("adds both generic and claimed when both missing", () => {
      const issue = makeIssue({
        number: 1,
        labels: labelNames("bounty: 💰 $300"),
        assignees: [{ login: "alice" }],
      });
      const patch = computeIssuePatch(issueState(issue));
      assert.equal(patch.ops.length, 1);
      assert.deepEqual(patch.ops[0]!.add.sort(), [BOUNTY_CLAIMED, BOUNTY_GENERIC].sort());
      assert.deepEqual(patch.ops[0]!.remove, []);
    });

    it("produces no patch when generic and claimed both already present", () => {
      const issue = makeIssue({
        number: 1,
        labels: labelNames("bounty: 💰 $300", BOUNTY_GENERIC, BOUNTY_CLAIMED),
        assignees: [{ login: "alice" }],
      });
      const patch = computeIssuePatch(issueState(issue));
      assert.deepEqual(patch.ops, []);
    });

    it("preserves non-bounty labels untouched", () => {
      const issue = makeIssue({
        number: 1,
        labels: labelNames("type: bug", "bounty: 💰 $100", BOUNTY_GENERIC, BOUNTY_CLAIMED),
        assignees: [{ login: "alice" }],
      });
      const patch = computeIssuePatch(issueState(issue));
      assert.deepEqual(patch.ops, []);
    });
  });

  describe("rewarded state", () => {
    it("leaves rewarded label alone (owned by PR sync)", () => {
      const issue = makeIssue({
        number: 1,
        labels: labelNames("bounty: 💰 $500", BOUNTY_GENERIC, BOUNTY_REWARDED),
        assignees: [],
      });
      const patch = computeIssuePatch(issueState(issue));
      // rewarded is not touched; only generic should be kept since value label present
      assert.deepEqual(patch.ops, []);
    });
  });
});

// ---------------------------------------------------------------------------
// linkedIssueNumbers
// ---------------------------------------------------------------------------

describe("linkedIssueNumbers", () => {
  it("returns empty array for null body", () => {
    assert.deepEqual(linkedIssueNumbers(null), []);
  });

  it("parses closes/fixes/resolves keywords", () => {
    const body = "Closes #10\nFixes #20\nResolves #30";
    assert.deepEqual(linkedIssueNumbers(body), [10, 20, 30]);
  });

  it("is case-insensitive", () => {
    const body = "CLOSES #5 fixes #6 RESOLVES #7";
    assert.deepEqual(linkedIssueNumbers(body), [5, 6, 7]);
  });

  it("handles plural forms: close/fix/resolve", () => {
    const body = "close #1 fix #2 resolve #3";
    assert.deepEqual(linkedIssueNumbers(body), [1, 2, 3]);
  });

  it("returns empty when no keywords present", () => {
    assert.deepEqual(linkedIssueNumbers("See issue #99 for context."), []);
  });
});

// ---------------------------------------------------------------------------
// computePrPatch — PR rules
// ---------------------------------------------------------------------------

describe("computePrPatch", () => {
  describe("label propagation (pre-merge)", () => {
    it("produces no patch when PR body has no linked issues", () => {
      const pr = makePr({ number: 10, body: "No references here." });
      const patch = computePrPatch({ pr, currentLabels: new Set(), linkedIssues: [] });
      assert.deepEqual(patch.ops, []);
    });

    it("produces no patch when linked issues have no bounty labels", () => {
      const pr = makePr({ number: 10, body: "Closes #1" });
      const issue = makeIssue({ number: 1, labels: labelNames("type: bug") });
      const patch = computePrPatch({ pr, currentLabels: new Set(), linkedIssues: [issue] });
      assert.deepEqual(patch.ops, []);
    });

    it("copies bounty value label from linked issue to PR", () => {
      const pr = makePr({ number: 10, body: "Closes #1" });
      const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $500") });
      const patch = computePrPatch({ pr, currentLabels: new Set(), linkedIssues: [issue] });

      const prOp = patch.ops.find((o) => o.target === 10);
      assert.ok(prOp);
      assert.deepEqual(prOp.add, ["bounty: 💰 $500"]);
      assert.deepEqual(prOp.remove, []);
    });

    it("does not duplicate a value label already on the PR", () => {
      const pr = makePr({ number: 10, body: "Closes #1" });
      const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $500") });
      const patch = computePrPatch({
        pr,
        currentLabels: new Set(["bounty: 💰 $500"]),
        linkedIssues: [issue],
      });
      assert.deepEqual(patch.ops, []);
    });

    it("posts a comment on the linked issue when label is propagated", () => {
      const pr = makePr({ number: 10, body: "Closes #1" });
      const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $200") });
      const patch = computePrPatch({ pr, currentLabels: new Set(), linkedIssues: [issue] });

      const issueOp = patch.ops.find((o) => o.target === 1);
      assert.ok(issueOp);
      assert.ok(issueOp.comment?.includes("PR"));
      assert.ok(issueOp.comment?.includes("#10"));
      assert.ok(issueOp.comment?.includes("contributor"));
    });

    it("does not post a comment when label was already on the PR", () => {
      const pr = makePr({ number: 10, body: "Closes #1" });
      const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $200") });
      const patch = computePrPatch({
        pr,
        currentLabels: new Set(["bounty: 💰 $200"]),
        linkedIssues: [issue],
      });
      const issueOp = patch.ops.find((o) => o.target === 1);
      assert.ok(!issueOp);
    });

    it("merges value labels from multiple linked issues onto the PR", () => {
      const pr = makePr({ number: 10, body: "Closes #1\nCloses #2" });
      const issue1 = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $100") });
      const issue2 = makeIssue({ number: 2, labels: labelNames("bounty: 💰 $200") });
      const patch = computePrPatch({
        pr,
        currentLabels: new Set(),
        linkedIssues: [issue1, issue2],
      });

      const prOp = patch.ops.find((o) => o.target === 10);
      assert.ok(prOp);
      assert.deepEqual(prOp.add.sort(), ["bounty: 💰 $100", "bounty: 💰 $200"].sort());
    });
  });

  describe("rewarded on merge", () => {
    it("produces no patch when PR was closed but not merged", () => {
      const pr = makePr({ number: 10, state: "closed", merged: false, body: "Closes #1" });
      const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $500") });
      const patch = computePrPatch({
        pr,
        currentLabels: new Set(["bounty: 💰 $500"]),
        linkedIssues: [issue],
      });
      const prOp = patch.ops.find((o) => o.target === 10);
      assert.ok(!prOp?.add.includes(BOUNTY_REWARDED));
    });

    it("adds rewarded to PR on merge", () => {
      const pr = makePr({
        number: 10,
        merged: true,
        body: "Closes #1",
        labels: labelNames("bounty: 💰 $500"),
      });
      const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $500") });
      const patch = computePrPatch({
        pr,
        currentLabels: new Set(["bounty: 💰 $500"]),
        linkedIssues: [issue],
      });

      const prOp = patch.ops.find((o) => o.target === 10);
      assert.ok(prOp);
      assert.ok(prOp.add.includes(BOUNTY_REWARDED));
    });

    it("adds rewarded to linked issue on merge", () => {
      const pr = makePr({
        number: 10,
        merged: true,
        body: "Closes #1",
        labels: labelNames("bounty: 💰 $500"),
      });
      const issue = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $500", BOUNTY_CLAIMED) });
      const patch = computePrPatch({
        pr,
        currentLabels: new Set(["bounty: 💰 $500"]),
        linkedIssues: [issue],
      });

      const issueOp = patch.ops.find((o) => o.target === 1);
      assert.ok(issueOp);
      assert.ok(issueOp.add.includes(BOUNTY_REWARDED));
      assert.ok(issueOp.remove.includes(BOUNTY_CLAIMED));
    });

    it("does not add rewarded again when already present", () => {
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
      const patch = computePrPatch({
        pr,
        currentLabels: new Set(["bounty: 💰 $500", BOUNTY_REWARDED]),
        linkedIssues: [issue],
      });
      assert.deepEqual(patch.ops, []);
    });

    it("skips issues with no bounty label when merged PR has a bounty", () => {
      const pr = makePr({
        number: 10,
        merged: true,
        body: "Closes #1\nCloses #2",
        labels: labelNames("bounty: 💰 $500"),
      });
      const issue1 = makeIssue({ number: 1, labels: labelNames("bounty: 💰 $500") });
      const issue2 = makeIssue({ number: 2, labels: labelNames("type: bug") });
      const patch = computePrPatch({
        pr,
        currentLabels: new Set(["bounty: 💰 $500"]),
        linkedIssues: [issue1, issue2],
      });

      // issue2 has no bounty so its desired state equals current: no op
      const issue2Op = patch.ops.find((o) => o.target === 2);
      assert.ok(!issue2Op);
    });
  });
});
