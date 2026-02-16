<operating_system>{{env.os}}</operating_system>
<current_working_directory>{{env.cwd}}</current_working_directory>
<default_shell>{{env.shell}}</default_shell>
<home_directory>{{env.home}}</home_directory>
{{#if files}}
<file_list>
{{#each files}} - {{path}}{{#if is_dir}}/{{/if}}
{{/each}}</file_list>
{{/if}}
{{#if extensions}}
<workspace_extensions command="git ls-files">
Summary: {{extensions.git_tracked_files}} total files
File distribution by extension (percentages show share of total workspace files):
{{#each extensions.extension_stats}} - {{extension}} ({{count}} files, {{percentage}}%)
{{/each}}{{#if (gt extensions.total_extensions extensions.max_extensions)}}
Note: Showing top {{extensions.max_extensions}} of {{extensions.total_extensions}} extensions.
{{/if}}</workspace_extensions>
{{/if}}