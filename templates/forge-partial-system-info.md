- **Operating System**: {{env.os}}
- **Current Working Directory**: {{env.cwd}}
- **Default Shell**: {{env.shell}}
- **Home Directory**: {{env.home}}
{{#if files}}

**File List:**
{{#each files}} - {{path}}{{#if is_dir}}/{{/if}}
{{/each}}{{/if}}
{{#if extensions}}

**Workspace Extensions** (`git ls-files` | {{extensions.git_tracked_files}} files | {{extensions.total_extensions}} extensions):
{{#each extensions.extension_stats}} - .{{extension}}: {{count}} files ({{percentage}}%)
{{/each}}{{#if (gt extensions.total_extensions extensions.max_extensions)}}(showing top {{extensions.max_extensions}} of {{extensions.total_extensions}} extensions; other extensions account for {{extensions.remaining_percentage}}% of files)
{{/if}}{{/if}}
