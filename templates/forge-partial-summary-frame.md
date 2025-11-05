Use the following summary frames as the authoritative reference for all coding suggestions and decisions. Do not re-explain or revisit it unless I ask. Additional summary frames will be added as the conversation progresses.

<summary_frame>
{{#each messages}}
<summary_block id="{{inc @index}}" role="{{role}}">
{{#each blocks}}
{{#if content}}{{content}}{{/if}}
{{~#if tool_call}}
<file_operations>
{{~#if tool_call.call.file_update}}<modified>{{tool_call.call.file_update.path}}</modified>{{/if~}}
{{~#if tool_call.call.file_read}}<read>{{tool_call.call.file_read.path}}</read>{{/if~}}
{{~#if tool_call.call.file_remove}}<deleted>{{tool_call.call.file_remove.path}}</deleted>{{/if~}}
</file_operations>
{{/if~}}
{{/each}}
</summary_block>
{{/each}}
</summary_frame>

Proceed with implementation based on this context.
