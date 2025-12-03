#!/bin/bash
# Simplified container entrypoint that runs task-executor directly
# Configuration passed via environment variables

set -e

# Install node_modules if not present
if [ ! -d "/workspace/benchmarks/node_modules" ]; then
  echo "Installing node_modules..."
  cd /workspace/benchmarks
  npm ci --production --silent
fi

cd /workspace/benchmarks

# Create debug directory at /tmp/debug to match context_input paths
mkdir -p /tmp/debug

# Build task configuration using Node.js (avoids bash/jq escaping issues)
node task-config-builder.js /tmp/task-config.json

# Run task-executor with JSON output mode
node dist/task-executor-cli.js /tmp/task-config.json
EXIT_CODE=$?

# Cleanup
rm -f /tmp/task-config.json

exit $EXIT_CODE
