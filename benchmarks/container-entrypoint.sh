#!/bin/bash
# Container entrypoint that uses task-executor for early exit support

set -e

# Install node_modules if not present
if [ ! -d "/workspace/benchmarks/node_modules" ]; then
  echo "Installing node_modules..."
  cd /workspace/benchmarks
  npm ci --production --silent
fi

# If TASK_COMMAND is set, use task-executor to run it
if [ -n "$TASK_COMMAND" ]; then
  # Write task config to temp file using jq to properly escape JSON
  TASK_CONFIG_FILE=$(mktemp)
  
  jq -n \
    --arg cmd "$TASK_COMMAND" \
    --argjson timeout "${TASK_TIMEOUT:-180}" \
    --argjson early_exit "${TASK_EARLY_EXIT:-false}" \
    --argjson validations "${TASK_VALIDATIONS:-[]}" \
    --argjson context "${TASK_CONTEXT:-{}}" \
    '{
      command: $cmd,
      timeout: $timeout,
      early_exit: $early_exit,
      validations: $validations,
      context: $context
    }' > "$TASK_CONFIG_FILE"
  
  # Run using compiled task-executor
  cd /workspace/benchmarks
  node dist/task-executor-runner.js "$TASK_CONFIG_FILE"
  EXIT_CODE=$?
  
  # Print the log file contents before exiting so Cloud Run captures it
  echo "================================================================================"
  echo "TASK EXECUTION LOG:"
  echo "================================================================================"
  if [ -f "/workspace/debug/task_run.log" ]; then
    cat /workspace/debug/task_run.log
  else
    echo "(No log file found)"
  fi
  echo "================================================================================"
  
  # Output log file as base64 for reliable extraction
  echo "<<<LOG_FILE_START>>>"
  if [ -f "/workspace/debug/task_run.log" ]; then
    base64 /workspace/debug/task_run.log
  fi
  echo "<<<LOG_FILE_END>>>"
  
  # Ensure all output is flushed to Cloud Logging
  sync
  sleep 1
  
  rm -f "$TASK_CONFIG_FILE"
  exit $EXIT_CODE
else
  # No TASK_COMMAND, just exec the args
  exec "$@"
fi
