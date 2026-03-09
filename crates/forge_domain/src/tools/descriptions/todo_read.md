Retrieves the current todo list for this coding session. Use this tool to check existing todos before making updates, or to review the current state of tasks at any point during the session.

## When to Use This Tool

- Before calling `todo_write`, to understand which tasks already exist and avoid duplicates
- When you need to know what tasks are pending, in progress, or completed
- To resume work after a break and understand the current state of tasks
- When the user asks about the current task list or progress

## Output

Returns all current todos with their IDs, content, and status (`pending`, `in_progress`, `completed`). If no todos exist yet, returns an empty list.
