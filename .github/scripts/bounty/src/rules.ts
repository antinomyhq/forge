// Rules engine for v2 bounty sync.
//
// Both exported functions are pure: they take current state and return the
// minimal Patch needed to reconcile it with the desired state.
// No I/O, no side effects — fully unit-testable without mocks.

import type { IssueState, PrState, Patch, LabelOp, Issue } from "./types.js";

// ---------------------------------------------------------------------------
// Label name constants
// ---------------------------------------------------------------------------

export const BOUNTY_GENERIC = "bounty";
export const BOUNTY_CLAIMED = "bounty: claimed";
export const BOUNTY_REWARDED = "bounty: rewarded";

/// Matches bounty value labels in both formats:
/// - new: "bounty: 💰 $100", "bounty: 💰 $5500"
/// - legacy: "bounty: $300", "bounty: $500"
export const VALUE_LABEL_RE = /^bounty: (?:💰 )?\$\d/;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns true when the label name is a bounty value label.
export function isBountyValue(label: string): boolean {
  return VALUE_LABEL_RE.test(label);
}

/// Extract all issue numbers referenced by closing keywords in a PR body.
/// A new RegExp is created on each call to avoid stale `lastIndex` state.
export function linkedIssueNumbers(body: string | null): number[] {
  const numbers: number[] = [];
  const text = body ?? "";
  const re = /(?:closes?|fix(?:es)?|resolves?)\s+#(\d+)/gi;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    numbers.push(parseInt(m[1]!, 10));
  }
  return numbers;
}

/// Build a LabelOp that only includes labels that actually need to change.
function diff(
  target: number,
  current: Set<string>,
  desired: Set<string>,
  comment?: string,
  meta?: { title?: string; url?: string }
): LabelOp | null {
  const add = [...desired].filter((l) => !current.has(l));
  const remove = [...current].filter((l) => !desired.has(l));
  if (add.length === 0 && remove.length === 0 && !comment) return null;
  return { target, title: meta?.title, url: meta?.url, add, remove, comment };
}

// ---------------------------------------------------------------------------
// Issue rules
// ---------------------------------------------------------------------------

/// Compute the desired label set for an issue given its full current state.
///
/// Rules (all applied simultaneously, not sequentially):
///
/// 1. **Generic bounty** — `bounty` must be present iff any value label is present.
/// 2. **Claimed** — `bounty: claimed` must be present iff the issue has a value
///    label AND at least one assignee. It must be absent otherwise.
/// 3. **Value labels and `bounty: rewarded`** — these are set by maintainers or
///    the PR merge job and are left untouched by the issue sync.
/// 4. All non-bounty labels are preserved as-is.
export function computeIssuePatch({ issue, currentLabels }: IssueState): Patch {
  const desired = new Set(currentLabels);

  const hasValueLabel = [...currentLabels].some(isBountyValue);
  const isAssigned = issue.assignees.length > 0;

  // Rule 1 — generic label mirrors value label presence.
  if (hasValueLabel) {
    desired.add(BOUNTY_GENERIC);
  } else {
    desired.delete(BOUNTY_GENERIC);
  }

  // Rule 2 — claimed mirrors assignment presence (only when bounty exists).
  if (hasValueLabel && isAssigned) {
    desired.add(BOUNTY_CLAIMED);
  } else {
    desired.delete(BOUNTY_CLAIMED);
  }

  const op = diff(issue.number, currentLabels, desired, undefined, {
    title: issue.title,
    url: issue.html_url,
  });
  return { ops: op ? [op] : [] };
}

// ---------------------------------------------------------------------------
// PR rules
// ---------------------------------------------------------------------------

/// Compute the desired state for a PR and its linked issues.
///
/// Rules:
///
/// 1. **Label propagation** — for each linked issue that has a value label,
///    copy that value label to the PR (if the PR doesn't already have it).
/// 2. **Rewarded on merge** — if the PR is merged and has a value label:
///    a. Add `bounty: rewarded` to the PR.
///    b. Add `bounty: rewarded` to each linked issue.
///    c. Remove `bounty: claimed` from each linked issue.
/// 3. **Comment on propagation** — when a value label is first added to the PR
///    via rule 1, post a comment on the source issue.
export function computePrPatch({ pr, currentLabels, linkedIssues }: PrState): Patch {
  const ops: LabelOp[] = [];

  const prDesired = new Set(currentLabels);

  // Collect value labels to propagate from linked issues.
  const labelsToPropagate: string[] = [];
  for (const issue of linkedIssues) {
    const issueValueLabels = issue.labels.map((l) => l.name).filter(isBountyValue);
    for (const label of issueValueLabels) {
      if (!prDesired.has(label)) {
        labelsToPropagate.push(label);
        prDesired.add(label);
      }
    }
  }

  const prHasValueLabel = [...prDesired].some(isBountyValue);

  if (pr.merged && prHasValueLabel) {
    // Rule 2 — rewarded lifecycle.
    prDesired.add(BOUNTY_REWARDED);

    const prOp = diff(pr.number, currentLabels, prDesired, undefined, {
      title: pr.title,
      url: pr.html_url,
    });
    if (prOp) ops.push(prOp);

    // Update linked issues — only those that already have a bounty value label.
    for (const issue of linkedIssues) {
      const issueCurrent = new Set(issue.labels.map((l) => l.name));
      const issueHasValue = [...issueCurrent].some(isBountyValue);
      if (!issueHasValue) continue;

      const issueDesired = new Set(issueCurrent);
      issueDesired.add(BOUNTY_REWARDED);
      issueDesired.delete(BOUNTY_CLAIMED);

      const op = diff(issue.number, issueCurrent, issueDesired, undefined, {
        title: issue.title,
        url: issue.html_url,
      });
      if (op) ops.push(op);
    }
  } else {
    // Rule 1 — label propagation (pre-merge).
    const prOp = diff(pr.number, currentLabels, prDesired, undefined, {
      title: pr.title,
      url: pr.html_url,
    });
    if (prOp) ops.push(prOp);

    // Rule 3 — comment on each issue whose value label was newly propagated.
    if (labelsToPropagate.length > 0) {
      for (const issue of linkedIssues) {
        const hadValueLabel = issue.labels.map((l) => l.name).some(isBountyValue);
        if (hadValueLabel) {
          ops.push({
            target: issue.number,
            title: issue.title,
            url: issue.html_url,
            add: [],
            remove: [],
            comment: `PR [#${pr.number}](${pr.html_url}) has been opened for this bounty by @${pr.user.login}.`,
          });
        }
      }
    }
  }

  return { ops };
}
