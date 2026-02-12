#!/bin/bash
# Create a GitHub issue with proper formatting and validation
# Usage: ./create_issue.sh [--title "Title"] [--body "Body"] [--label "label1,label2"] [--assignee "username"] [--milestone "number"] [--draft]

set -e

# Default values
TITLE=""
BODY=""
LABELS=""
ASSIGNEE=""
MILESTONE=""
DRAFT=false

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --title)
      TITLE="$2"
      shift 2
      ;;
    --body)
      BODY="$2"
      shift 2
      ;;
    --label)
      LABELS="$2"
      shift 2
      ;;
    --assignee)
      ASSIGNEE="$2"
      shift 2
      ;;
    --milestone)
      MILESTONE="$2"
      shift 2
      ;;
    --draft)
      DRAFT=true
      shift
      ;;
    *)
      echo "Unknown option: $1"
      echo "Usage: $0 [--title \"Title\"] [--body \"Body\"] [--label \"label1,label2\"] [--assignee \"username\"] [--milestone \"number\"] [--draft]"
      exit 1
      ;;
  esac
done

# Validate required fields
if [ -z "$TITLE" ]; then
  echo "Error: --title is required"
  exit 1
fi

# Build gh issue command
CMD="gh issue create --title \"$TITLE\""

if [ -n "$BODY" ]; then
  CMD="$CMD --body \"$BODY\""
fi

if [ -n "$LABELS" ]; then
  CMD="$CMD --label \"$LABELS\""
fi

if [ -n "$ASSIGNEE" ]; then
  CMD="$CMD --assignee \"$ASSIGNEE\""
fi

if [ -n "$MILESTONE" ]; then
  CMD="$CMD --milestone \"$MILESTONE\""
fi

if [ "$DRAFT" = true ]; then
  CMD="$CMD --draft"
fi

# Execute command
eval $CMD