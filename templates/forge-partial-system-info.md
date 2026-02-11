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
<file_extensions>
{{#each extensions}} - {{count}} .{{extension}} files
{{/each}}</file_extensions>
{{/if}}