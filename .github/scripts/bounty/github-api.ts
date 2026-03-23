// Shared types and GitHub API abstraction for bounty scripts.

export interface Label {
  name: string;
}

export interface Assignee {
  login: string;
}

export interface PullRequestEvent {
  pull_request: {
    number: number;
    merged: boolean;
    body: string | null;
    html_url: string;
    labels: Label[];
    user: { login: string };
  };
}

export interface IssuesEvent {
  action: string;
  /// The label that was added or removed — present on `labeled` and `unlabeled` actions only.
  label?: Label;
  issue: {
    number: number;
    labels: Label[];
    assignees: Assignee[];
  };
}

// ---------------------------------------------------------------------------
// Pipeline types
// ---------------------------------------------------------------------------

/// A mutation to apply to a single issue or PR number.
/// Labels to add are batched into one API call. Each removal is a separate call
/// (GitHub has no bulk remove endpoint).
export interface TargetMutation {
  target: number;
  /// Labels to add in a single batched API call.
  add: string[];
  /// Labels to remove, one call each.
  remove: string[];
  /// Comments to post, one call each.
  comment?: string;
}

/// Output of the parse step. Describes what the script *wants* to happen,
/// before any API state is known.
///
/// `knownLabels` carries label sets already present in the event payload
/// (so the plan step can skip fetching those). Keys are issue/PR numbers.
///
/// `labelCopies` is used when labels must be copied from source issues to a
/// target PR — the plan step fetches source labels and merges them.
export interface ParsedIntent {
  mutations: TargetMutation[];
  /// Label sets already known from the event payload, keyed by target number.
  /// The plan step uses these to avoid redundant GET calls.
  knownLabels: Record<number, string[]>;
  /// Copy bounty labels from source issues onto a PR target.
  /// The plan step fetches each source issue's labels and adds any matching
  /// bounty: $N labels to the PR mutation, deduplicating against what the PR
  /// already has (supplied via knownLabels[prTarget]).
  labelCopies?: {
    sources: number[];
    prTarget: number;
  };
}

/// Output of the plan step. All redundant operations have been removed
/// (no-op adds/removes filtered out). Ready to execute.
export interface BatchPlan {
  mutations: TargetMutation[];
}

// ---------------------------------------------------------------------------
// GitHub REST API abstraction
// ---------------------------------------------------------------------------

/// Abstraction over the GitHub REST API — injectable for testing.
export interface GitHubApi {
  getLabels(issueOrPr: number): Promise<Label[]>;
  addLabels(issueOrPr: number, labels: string[]): Promise<void>;
  removeLabel(issueOrPr: number, label: string): Promise<void>;
  addComment(issue: number, body: string): Promise<void>;
}

import * as https from "https";

/// Production implementation that calls the real GitHub REST API.
export class GitHubRestApi implements GitHubApi {
  constructor(
    private readonly owner: string,
    private readonly repo: string,
    private readonly token: string
  ) {}

  private request<T>(method: string, path: string, body?: unknown): Promise<T> {
    return new Promise((resolve, reject) => {
      const payload = body ? JSON.stringify(body) : undefined;
      const options: https.RequestOptions = {
        hostname: "api.github.com",
        path,
        method,
        headers: {
          Authorization: `Bearer ${this.token}`,
          Accept: "application/vnd.github+json",
          "User-Agent": "bounty-bot",
          "X-GitHub-Api-Version": "2022-11-28",
          ...(payload ? { "Content-Type": "application/json" } : {}),
        },
      };
      const req = https.request(options, (res) => {
        let data = "";
        res.on("data", (chunk: string) => (data += chunk));
        res.on("end", () => {
          try {
            resolve(data ? (JSON.parse(data) as T) : ({} as T));
          } catch {
            resolve({} as T);
          }
        });
      });
      req.on("error", reject);
      if (payload) req.write(payload);
      req.end();
    });
  }

  async getLabels(issueOrPr: number): Promise<Label[]> {
    return this.request<Label[]>(
      "GET",
      `/repos/${this.owner}/${this.repo}/issues/${issueOrPr}/labels`
    );
  }

  async addLabels(issueOrPr: number, labels: string[]): Promise<void> {
    await this.request(
      "POST",
      `/repos/${this.owner}/${this.repo}/issues/${issueOrPr}/labels`,
      { labels }
    );
  }

  async removeLabel(issueOrPr: number, label: string): Promise<void> {
    await this.request(
      "DELETE",
      `/repos/${this.owner}/${this.repo}/issues/${issueOrPr}/labels/${encodeURIComponent(label)}`
    );
  }

  async addComment(issue: number, body: string): Promise<void> {
    await this.request(
      "POST",
      `/repos/${this.owner}/${this.repo}/issues/${issue}/comments`,
      { body }
    );
  }
}
