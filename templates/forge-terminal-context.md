<!-- FIXME: Drop this file -->
{{#each commands}}- `{{command}}` [{{#if exit_code}}FAILED (exit {{exit_code}}){{else}}ok{{/if}}] @ {{timestamp}}
{{/each}}