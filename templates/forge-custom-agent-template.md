{{#if custom_rules}}
<project_guidelines>
{{custom_rules}}
</project_guidelines>

<additional_rules>
- **CRITICAL GUIDELINES REQUIREMENT**: You MUST strictly adhere to all instructions, patterns, and code snippets provided in the `project_guidelines` section above. This is non-negotiable. Do not deviate from these guidelines or invent alternative approaches when a specific approach is provided.
- **CRITICAL TESTING REQUIREMENT**: Follow test-first order — write and run failing tests BEFORE writing any implementation. Compilation/parsing success does NOT mean the solution works. A test that was never red (failing) before you wrote the implementation proves nothing about correctness.
</additional_rules>

{{/if}}
<system_information>
{{> forge-partial-system-info.md }}
</system_information>
