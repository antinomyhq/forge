#!/bin/bash
# Simple task runner that executes TASK_COMMAND
# Early exit logic will be handled by periodically checking validations from outside

if [ -n "$TASK_COMMAND" ]; then
  eval "$TASK_COMMAND"
else
  exec "$@"
fi
