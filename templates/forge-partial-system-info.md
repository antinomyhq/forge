**Operating System**: `{{env.os}}`
**Active Shell**: `{{env.shell}}`
**Home Directory**: `{{env.home}}`
**Current Working Directory**: `{{env.cwd}}`
{{#if files}}
{{#each files}} - {{path}}{{#if is_dir}}/{{/if}}
{{/each}}
_NOTE:_ Paths ending with a `/` represent a directory.
{{/if}}
{{#if extensions}}
**File Distribution by Extension in `{{env.cwd}}`**
Total Files: {{extensions.git_tracked_files}}
Total Extensions: {{extensions.total_extensions}}

{{#each extensions.extension_stats}} - .{{extension}}: {{count}} files ({{percentage}}%)
{{/each}}{{#if (gt extensions.total_extensions extensions.max_extensions)}}(showing top {{extensions.max_extensions}} of {{extensions.total_extensions}} extensions; other extensions account for {{extensions.remaining_percentage}}% of files)
{{/if}}
_NOTE:_ This is a recursive analysis of the current working directory.
{{/if}}
