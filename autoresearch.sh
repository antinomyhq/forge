#!/usr/bin/env bash
set -euo pipefail

MODE="${FORGE_BENCH_MODE:-sync}"
FILE_COUNT="${FORGE_BENCH_FILE_COUNT:-400}"
FILE_BYTES="${FORGE_BENCH_FILE_BYTES:-524288}"
BATCH_SIZE="${FORGE_BENCH_BATCH_SIZE:-64}"

cargo check -p forge_services --example workspace_sync_memory >/dev/null
cargo build -p forge_services --example workspace_sync_memory >/dev/null

stdout_file="$(mktemp)"
stderr_file="$(mktemp)"
trap 'rm -f "$stdout_file" "$stderr_file"' EXIT

FORGE_BENCH_MODE="$MODE" \
FORGE_BENCH_FILE_COUNT="$FILE_COUNT" \
FORGE_BENCH_FILE_BYTES="$FILE_BYTES" \
FORGE_BENCH_BATCH_SIZE="$BATCH_SIZE" \
/usr/bin/time -l target/debug/examples/workspace_sync_memory >"$stdout_file" 2>"$stderr_file"

if ! grep -q '^BENCHMARK_OK ' "$stdout_file"; then
  echo "benchmark failed" >&2
  cat "$stdout_file" >&2
  cat "$stderr_file" >&2
  exit 1
fi

rss_bytes="$(awk '/maximum resident set size/ { print $1; exit }' "$stderr_file")"
if [[ -z "$rss_bytes" ]]; then
  echo "failed to parse max RSS" >&2
  cat "$stderr_file" >&2
  exit 1
fi

rss_mb="$(python3 - <<'PY' "$rss_bytes"
import sys
rss_bytes = float(sys.argv[1])
print(f"{rss_bytes / (1024 * 1024):.2f}")
PY
)"

printf 'METRIC peak_rss_mb=%s\n' "$rss_mb"
tr '\n' ' ' < "$stdout_file"
printf '\n'
