<operating_system>{{env.os}}</operating_system>
<current_working_directory>{{env.cwd}}</current_working_directory>
<default_shell>{{env.shell}}</default_shell>
<home_directory>{{env.home}}</home_directory>
{{#if files}}
<file_list>
{{#each files}} - {{path}}{{#if is_dir}}/{{/if}}
{{/each}}</file_list>
{{/if}}

{{#if env.background}}
**IMPORTANT**: This directory information is provided for your convenience at the start of the task. You should:
- Use this information to understand the initial environment state
- Avoid redundantly calling `ls` or similar commands just to list the same directory
- Only use file listing commands if you need updated information (after creating/deleting files) or need to explore subdirectories
- Work in the /app directory unless explicitly instructed otherwise
{{/if}}