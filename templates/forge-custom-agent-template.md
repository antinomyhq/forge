{{#if custom_rules}}
<project_guidelines>
{{custom_rules}}
</project_guidelines>

<additional_rules>
- Always follow all the `project_guidelines` without exception.
- **CRITICAL TESTING REQUIREMENT**: Before marking any task as complete, you MUST verify functional correctness by running actual tests with real data. Many tasks fail because agents skip this verification step. Compilation/parsing success does NOT mean the solution works - always run functional tests.
</additional_rules>

{{/if}}
<system_information>
{{> forge-partial-system-info.md }}
</system_information>
