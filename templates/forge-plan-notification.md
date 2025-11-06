<system-reminder>
{{#if plan.is_complete}}
## Plan Completed

**Plan:** `{{plan.path}}`

{{#if plan.tasks_failed}}
### Failed Tasks Require Attention
{{#each plan.tasks_failed}}
- [!] Line {{this.line_number}}: {{this.description}}
{{/each}}

**Next Steps:**
- Review each failed task and attempt to resolve issues
- Document blockers in the plan file if tasks cannot be completed
- Update status: `[!]` → `[x]` (resolved) or add blocker comment
{{else}}
All tasks completed successfully!
{{/if}}

{{else}}
## Active Plan In Progress

**Plan:** `{{plan.path}}`

{{#if plan.tasks_in_progress}}
**In Progress:**
{{#each plan.tasks_in_progress}}
- [~] Line {{this.line_number}}: {{this.description}}
{{/each}}
{{/if}}
{{#if plan.tasks_failed}}
**Failed:**
{{#each plan.tasks_failed}}
- [!] Line {{this.line_number}}: {{this.description}}
{{/each}}
{{/if}}
{{#if plan.next_pending_task}}
**Next:** [Line {{plan.next_pending_task.line_number}}] {{plan.next_pending_task.description}}
{{/if}}

**Instructions:**
- Prioritize blocked or critical failed tasks first
- Otherwise continue in-progress tasks, then start next pending
- Update status in the plan file: `[ ]` (pending) → `[~]` (working) → `[x]` (done)
- Keep `[!]` for failed tasks until resolved
{{/if}}
</system-reminder>
