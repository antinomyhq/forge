#!/bin/sh
cd '/app/workspaces/4a2ab07b-4e45-40e5-9bae-086ee16bbd30' || exit 1
exec 'claude' '--dangerously-skip-permissions' '--print' '/humanize:start-rlcr-loop docs/plan.md --max 3 --yolo --codex-model claude-sonnet-4.6:high --full-review-round 3 --track-plan-file'
