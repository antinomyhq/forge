// GitHub API abstraction for v2 bounty sync.
// Uses the native `fetch` API (available in Node 18+).

import type { Issue, Label, PullRequest } from "./types.js";

// ---------------------------------------------------------------------------
// Interface — injectable for testing
// ---------------------------------------------------------------------------

/// Minimal GitHub REST API surface needed by the bounty sync commands.
export interface GitHubApi {
  /// Fetch a full issue by number. Throws on non-2xx.
  getIssue(number: number): Promise<Issue>;
  /// Fetch a full pull request by number. Throws on non-2xx.
  getPullRequest(number: number): Promise<PullRequest>;
  /// Fetch all open issues that have any label matching the given prefix.
  /// Handles pagination automatically and excludes pull requests.
  listIssuesWithLabelPrefix(prefix: string): Promise<Issue[]>;
  /// Add one or more labels to an issue or PR. Batched into a single request.
  addLabels(target: number, labels: string[]): Promise<void>;
  /// Remove a single label from an issue or PR.
  removeLabel(target: number, label: string): Promise<void>;
  /// Post a comment on an issue or PR.
  addComment(target: number, body: string): Promise<void>;
}

// ---------------------------------------------------------------------------
// Production implementation
// ---------------------------------------------------------------------------

/// Calls the real GitHub REST API v3 using `fetch`.
export class GitHubRestApi implements GitHubApi {
  private readonly base: string;
  private readonly headers: Record<string, string>;

  constructor(
    private readonly owner: string,
    private readonly repo: string,
    token: string
  ) {
    this.base = `https://api.github.com/repos/${owner}/${repo}`;
    this.headers = {
      Authorization: `Bearer ${token}`,
      Accept: "application/vnd.github+json",
      "X-GitHub-Api-Version": "2022-11-28",
      "User-Agent": "bounty-bot/v2",
      "Content-Type": "application/json",
    };
  }

  private async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const res = await fetch(`${this.base}${path}`, {
      method,
      headers: this.headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });

    if (!res.ok) {
      const text = await res.text().catch(() => "");
      throw new Error(`GitHub API ${method} ${path} → ${res.status}: ${text}`);
    }

    // 204 No Content
    if (res.status === 204) return {} as T;

    return res.json() as Promise<T>;
  }

  async getIssue(number: number): Promise<Issue> {
    return this.request<Issue>("GET", `/issues/${number}`);
  }

  async getPullRequest(number: number): Promise<PullRequest> {
    return this.request<PullRequest>("GET", `/pulls/${number}`);
  }

  async listIssuesWithLabelPrefix(prefix: string): Promise<Issue[]> {
    const results: Issue[] = [];
    let page = 1;
    while (true) {
      const batch = await this.request<Issue[]>(
        "GET",
        `/issues?state=open&per_page=100&page=${page}`
      );
      if (batch.length === 0) break;
      for (const issue of batch) {
        // Exclude pull requests (GitHub returns them in /issues)
        if (issue.pull_request !== undefined) continue;
        if (issue.labels.some((l) => l.name.startsWith(prefix))) {
          results.push(issue);
        }
      }
      if (batch.length < 100) break;
      page++;
    }
    return results;
  }

  async addLabels(target: number, labels: string[]): Promise<void> {
    await this.request("POST", `/issues/${target}/labels`, { labels });
  }

  async removeLabel(target: number, label: string): Promise<void> {
    await this.request("DELETE", `/issues/${target}/labels/${encodeURIComponent(label)}`);
  }

  async addComment(target: number, body: string): Promise<void> {
    await this.request("POST", `/issues/${target}/comments`, { body });
  }
}
