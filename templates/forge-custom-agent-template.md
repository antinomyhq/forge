<additional_rules>
{{#if env.taskTimeoutSecs}}{{#if (gt env.taskTimeoutSecs 900)}}
- **CRITICAL TESTING REQUIREMENT**: Follow test-first order — write and run failing tests BEFORE writing any implementation. Write one test per requirement category, no duplicates. After tests pass, verify the tests actually exercise the requirement by checking they test real behavior, not trivial stubs.
{{else}}
- **CRITICAL TESTING REQUIREMENT**: Produce a working artifact at the final output path first. Then add one targeted executable check per requirement category — no duplicates. Do not spend more than 15% of the budget on tests before the first working artifact exists.
{{/if}}{{else}}
- **CRITICAL TESTING REQUIREMENT**: Produce a working artifact at the final output path first. Then add one targeted executable check per requirement category — no duplicates. For isolated logic bugs where the failing case is obvious, write a minimal failing test first.
{{/if}}
</additional_rules>


<system_information>
{{> forge-partial-system-info.md }}
</system_information>
