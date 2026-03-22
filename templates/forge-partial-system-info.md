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
<workspace_extensions command="git ls-files" files="{{extensions.git_tracked_files}}" extensions="{{extensions.total_extensions}}">
{{#each extensions.extension_stats}} - .{{extension}}: {{count}} files ({{percentage}}%)
{{/each}}{{#if (gt extensions.total_extensions extensions.max_extensions)}}(showing top {{extensions.max_extensions}} of {{extensions.total_extensions}} extensions; other extensions account for {{extensions.remaining_percentage}}% of files)
{{/if}}</workspace_extensions>
{{/if}}