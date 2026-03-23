#!/usr/bin/env bash
# Validates release notes piped via stdin or passed as a file argument.
# Usage:
#   echo "..." | bash validate-release-notes.sh
#   bash validate-release-notes.sh release-notes.md
#
# Exit codes:
#   0 — valid (under 2000 characters)
#   1 — invalid (2000 characters or over)

set -euo pipefail

MAX_CHARS=2000

if [[ $# -ge 1 ]]; then
  content=$(cat "$1")
else
  content=$(cat)
fi

char_count=${#content}

if [[ $char_count -lt $MAX_CHARS ]]; then
  echo "PASS: $char_count characters (limit: $MAX_CHARS)"
  exit 0
else
  echo "FAIL: $char_count characters — exceeds limit of $MAX_CHARS" >&2
  echo "Trim $(($char_count - $MAX_CHARS)) more character(s) to pass." >&2
  exit 1
fi
