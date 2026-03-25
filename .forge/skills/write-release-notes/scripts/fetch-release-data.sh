#!/usr/bin/env bash
# Fetches all PR numbers from a GitHub release and outputs their details.
# Usage: ./fetch-release-data.sh <version> [repo]
# Example: ./fetch-release-data.sh v1.32.0 antinomyhq/forge

set -euo pipefail

VERSION="${1:?Usage: $0 <version> [repo]}"
REPO="${2:-$(gh repo view --json nameWithOwner -q '.nameWithOwner' 2>/dev/null)}"

if [[ -z "$REPO" ]]; then
  echo "ERROR: Could not determine repository. Pass it as second argument." >&2
  exit 1
fi

# Use a temp file to avoid large variable issues with bash subshells
TMPFILE=$(mktemp)
trap 'rm -f "$TMPFILE"' EXIT

# Fetch release metadata (strip ANSI color codes that gh CLI may inject)
gh api "repos/$REPO/releases/tags/$VERSION" | sed 's/\x1b\[[0-9;]*m//g' > "$TMPFILE"

if [[ ! -s "$TMPFILE" ]]; then
  echo "ERROR: Release $VERSION not found in $REPO" >&2
  exit 1
fi

echo "### RELEASE METADATA ###"
jq '{tagName: .tag_name, publishedAt: .published_at, releaseName: .name, body: .body}' < "$TMPFILE"

# Extract PR numbers from the release body (handle \r\n line endings)
PR_NUMBERS=$(jq -r '.body // ""' < "$TMPFILE" | tr -d '\r' | grep --color=never -oE '#[0-9]+' | tr -d '#' | sort -un)

if [[ -z "$PR_NUMBERS" ]]; then
  echo "WARNING: No PR numbers found in release body." >&2
  exit 0
fi

echo "### PR DETAILS ###"
for PR_NUM in $PR_NUMBERS; do
  PR_DATA=$(gh pr view "$PR_NUM" \
    --repo "$REPO" \
    --json number,title,body,labels,author,mergedAt,url \
    2>/dev/null | sed 's/\x1b\[[0-9;]*m//g') || true
  if [[ -n "$PR_DATA" ]]; then
    echo "$PR_DATA"
  else
    echo "{\"number\": $PR_NUM, \"error\": \"not found\"}"
  fi
done
