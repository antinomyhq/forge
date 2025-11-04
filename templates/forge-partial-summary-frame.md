Use the following summary as the authoritative reference for all coding suggestions and decisions. Do not re-explain or revisit it unless I ask.

---
{{#if messages}}
## Summary
{{#each messages}}
###{{@index}} - {{role}}
{{#if messages}}{{#each messages}}{{#if content}}- _"{{content}}"_
{{/if}}{{#if tool_call.file_read}}- Read: `{{tool_call.file_read.path}}`{{/if}}{{#if tool_call.file_update}}- Updated: `{{tool_call.file_update.path}}`{{/if}}{{#if tool_call.file_remove}}- Deleted: `{{tool_call.file_remove.path}}`{{/if}}
{{/each}}{{/if}}
{{/each}}
{{/if}}
---

Proceed with implementation based on this context.
