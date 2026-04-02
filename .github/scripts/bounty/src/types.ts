// Core domain types for v2 bounty sync.
//
// Design: both sync commands work by fetching the complete current state of an
// issue or PR from GitHub, deriving the *desired* label set from the rules, and
// computing a minimal patch (add / remove) to reconcile the two.

// ---------------------------------------------------------------------------
// GitHub resource types (as returned by the REST API)
// ---------------------------------------------------------------------------

export interface Label {
  name: string;
}

export interface User {
  login: string;
}

/// Full issue as returned by GET /repos/:owner/:repo/issues/:number
export interface Issue {
  number: number;
  title: string;
  html_url: string;
  state: "open" | "closed";
  labels: Label[];
  assignees: User[];
  pull_request?: unknown; // present when the issue is actually a PR
}

/// Full pull request as returned by GET /repos/:owner/:repo/pulls/:number
export interface PullRequest {
  number: number;
  title: string;
  state: "open" | "closed";
  merged: boolean;
  body: string | null;
  labels: Label[];
  user: User;
  html_url: string;
}

// ---------------------------------------------------------------------------
// Derived state used by the rules engine
// ---------------------------------------------------------------------------

/// Everything the rules engine needs to know about an issue.
export interface IssueState {
  issue: Issue;
  /// Current label names on the issue.
  currentLabels: Set<string>;
}

/// Everything the rules engine needs to know about a PR and its linked issues.
export interface PrState {
  pr: PullRequest;
  /// Current label names on the PR.
  currentLabels: Set<string>;
  /// Full state of each issue linked via "closes/fixes/resolves #N" in the PR body.
  linkedIssues: Issue[];
}

// ---------------------------------------------------------------------------
// Patch types — the minimal diff to apply
// ---------------------------------------------------------------------------

/// A label operation on a single target (issue or PR number).
export interface LabelOp {
  target: number;
  /// Human-readable title of the issue or PR, for display in plan output.
  title?: string;
  /// URL of the issue or PR, for display in plan output.
  url?: string;
  add: string[];
  remove: string[];
  /// Optional comment to post on the target after label ops.
  comment?: string;
}

/// The complete set of operations to apply.
export interface Patch {
  ops: LabelOp[];
}
