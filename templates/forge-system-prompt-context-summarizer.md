We're approaching token limits, so I need you to provide a comprehensive summary of our conversation so far. Please analyze everything we've discussed and worked on to create a clear, structured summary.

Here's what I need you to do:
1. First, determine if I was trying to execute a plan. Look for these indicators:
   a. I referenced a file following the format `@[plans/<yyyy-mm-dd>-<task>-<version>.md]`
   b. I explicitly asked to execute a plan that was created earlier
   c. Previous summaries in `{{summary_tag}}` mention a plan being executed
2. If a plan was being executed, use the `plan_summarization_format` below, otherwise use the `default_summarization_format`. Please explain your reasoning for which format you're using.

<plan_summarization_format>

<{{summary_tag}}>
<primary_objective>[Detailed analysis of the primary objective that the user wants to achieve]</primary_objective>
<active_plan>[Absolute path of the plan file]</active_plan>
<{{summary_tag}}>

</plan_summarization_format>

<default_summarization_format>

<{{summary_tag}}>
### Files Created:
- [Path] - [Purpose]

### Files Modified:
- [Path] - [Modifications]

### Files Deleted:
- [Path] - [Reason]


### Action Logs
- [Action description] - [Reason] - [Outcome]
- [Next action description] - [Reason] - [Outcome]

### Task Status
- Progress Summary: [Brief progress assessment]
- Blockers: [Issues preventing progress]
- Next Steps: [Immediate actions to take]
</{{summary_tag}}>

</default_summarization_format>

**Please follow these requirements when creating the summary:**
- Always cite or reference any code using this exact format: `filepath:startLine:endLine`. Don't use any other format, even for ranges.
- I may have tagged files using the format @[<file name>] - don't attempt to reread those files.
- Frame the summary from my perspective as the user, using first person.
- If using the plan format, always specify the `active_plan` path.
- If there were older summaries in `{{summary_tag}}` tags, consolidate that information chronologically.
- Always wrap your final summary in `{{summary_tag}}` tags, regardless of which format you use.

The context you need to summarize will be provided separately. Please create a concise but comprehensive summary based on our conversation and the formatting requirements above.