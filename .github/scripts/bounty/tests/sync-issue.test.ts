import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { syncIssue } from "../src/sync-issue.js";
import type { GitHubApi } from "../src/api.js";
import type { Issue, PullRequest } from "../src/types.js";

// ---------------------------------------------------------------------------
// Mock API
// ---------------------------------------------------------------------------

function makeMockApi(issue: Issue): GitHubApi & {
  added: string[][];
  removed: string[];
  comments: string[];
} {
  const added: string[][] = [];
  const removed: string[] = [];
  const comments: string[] = [];

  return {
    added,
    removed,
    comments,
    async getIssue() {
      return issue;
    },
    async getPullRequest(): Promise<PullRequest> {
      throw new Error("not used");
    },
    async listIssuesWithLabelPrefix(): Promise<Issue[]> {
      throw new Error("not used");
    },
    async addLabels(_target, labels) {
      added.push(labels);
    },
    async removeLabel(_target, label) {
      removed.push(label);
    },
    async addComment(_target, body) {
      comments.push(body);
    },
  };
}

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

function labelNames(...names: string[]): { name: string }[] {
  return names.map((name) => ({ name }));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("syncIssue", () => {
  it("returns empty patch and makes no API calls when issue is already in sync", async () => {
    const issue = makeIssue({
      number: 42,
      labels: labelNames("bounty: 💰 $500", "bounty", "bounty: claimed"),
      assignees: [{ login: "alice" }],
    });
    const api = makeMockApi(issue);
    const patch = await syncIssue({ issueNumber: 42, api });

    assert.deepEqual(patch.ops, []);
    assert.deepEqual(api.added, []);
    assert.deepEqual(api.removed, []);
  });

  it("adds generic bounty label when value label is present but generic is missing", async () => {
    const issue = makeIssue({
      number: 42,
      labels: labelNames("bounty: 💰 $300"),
    });
    const api = makeMockApi(issue);
    const patch = await syncIssue({ issueNumber: 42, api });

    assert.equal(patch.ops.length, 1);
    assert.deepEqual(api.added, [["bounty"]]);
    assert.deepEqual(api.removed, []);
  });

  it("adds both generic and claimed when value label present and issue assigned", async () => {
    const issue = makeIssue({
      number: 42,
      labels: labelNames("bounty: 💰 $800"),
      assignees: [{ login: "bob" }],
    });
    const api = makeMockApi(issue);
    await syncIssue({ issueNumber: 42, api });

    assert.equal(api.added.length, 1);
    assert.deepEqual(api.added[0]!.sort(), ["bounty", "bounty: claimed"].sort());
  });

  it("removes claimed when last assignee is removed", async () => {
    const issue = makeIssue({
      number: 42,
      labels: labelNames("bounty: 💰 $500", "bounty", "bounty: claimed"),
      assignees: [],
    });
    const api = makeMockApi(issue);
    await syncIssue({ issueNumber: 42, api });

    assert.deepEqual(api.removed, ["bounty: claimed"]);
    assert.deepEqual(api.added, []);
  });

  it("removes both generic and claimed when value label is removed", async () => {
    const issue = makeIssue({
      number: 42,
      labels: labelNames("bounty", "bounty: claimed"),
      assignees: [{ login: "alice" }],
    });
    const api = makeMockApi(issue);
    await syncIssue({ issueNumber: 42, api });

    assert.deepEqual(api.removed.sort(), ["bounty", "bounty: claimed"].sort());
    assert.deepEqual(api.added, []);
  });

  it("does a single batched addLabels call for multiple adds", async () => {
    const issue = makeIssue({
      number: 42,
      labels: labelNames("bounty: 💰 $100"),
      assignees: [{ login: "carol" }],
    });
    const api = makeMockApi(issue);
    await syncIssue({ issueNumber: 42, api });

    // One call, two labels
    assert.equal(api.added.length, 1);
    assert.equal(api.added[0]!.length, 2);
  });
});
