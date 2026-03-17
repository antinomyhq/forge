{{#if custom_rules}}
<project_guidelines>
{{custom_rules}}
</project_guidelines>

<additional_rules>
- **CRITICAL GUIDELINES REQUIREMENT**: You MUST strictly adhere to all instructions, patterns, and code snippets provided in the `project_guidelines` section above. This is non-negotiable. Do not deviate from these guidelines or invent alternative approaches when a specific approach is provided.
- **CRITICAL TESTING REQUIREMENT**: Follow test-first order — write and run failing tests BEFORE writing any implementation. After tests pass, validate test quality by removing a key part of the implementation and confirming the tests now fail. If they still pass, the tests are too weak — re-examine the requirement and redesign the test to match the real execution context. Do NOT proceed with weak tests.
</additional_rules>

{{/if}}
<system_information>
{{> forge-partial-system-info.md }}
</system_information>
