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
<workspace_extensions>
{{#each extensions}} - {{extension}} ({{count}} files, {{percentage}}%)
{{/each}}{{#if (eq (len extensions) 15)}}
Note: Showing top 15 extensions only. Additional extensions exist but are not listed.
{{/if}}</workspace_extensions>
{{/if}}