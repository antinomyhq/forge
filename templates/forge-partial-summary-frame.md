Use the following summary frames as the authoritative reference for all coding suggestions and decisions. Do not re-explain or revisit it unless I ask. Additional summary frames will be added as the conversation progresses.

## Summary

{{#each messages}}
### {{inc @index}}. {{role}}

{{#each contents}}
{{#if text}}
````
{{text}}
````
{{/if}}
{{~#if tool_call}}
{{#if tool_call.tool.file_update}}
**Update:** `{{tool_call.tool.file_update.path}}`
{{else if tool_call.tool.file_read}}
**Read:** `{{tool_call.tool.file_read.path}}`
{{else if tool_call.tool.file_remove}}
**Delete:** `{{tool_call.tool.file_remove.path}}`
{{else if tool_call.tool.search}}
**Search:** `{{tool_call.tool.search.pattern}}`
{{else if tool_call.tool.skill}}
**Skill:** `{{tool_call.tool.skill.name}}`
{{else if tool_call.tool.sem_search}}
**Semantic Search:**
{{#each tool_call.tool.sem_search.queries}}
- `{{use_case}}`
{{/each}}
{{else if tool_call.tool.shell}}
**Execute:** 
```
{{tool_call.tool.shell.command}}
```
{{/if~}}
{{/if~}}

{{/each}}

{{/each}}

---

Proceed with implementation based on this context.
