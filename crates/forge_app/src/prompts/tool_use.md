{{#if (not tool_supported)}}

Tool Usage Instructions:

You have access to set of tools as described in the <available_tools> tag. You can use one tool per message, and will receive the result of that tool use in the user's response. You use tools step-by-step to accomplish a given task, with each tool use informed by the result of the previous tool use.

Tool Use Formatting Rules:

1. Tool use is formatted using XML-style tags ONLY.
2. You can only make one tool call per message.
3. Each tool call must be wrapped in `<tool_call>` tags.
4. The actual tool name (e.g., tool_forge_fs_read) must be used as the enclosing tag.
5. Each parameter must be enclosed within its own set of tags.

Important:

- ALWAYS use XML format, even for multiple operations
- Do NOT use JSON format (e.g., `tool_name({"param": "value"})`)
- Do NOT mix formats in the same message
- If you need to make multiple tool calls, send them in separate messages

{{> tool_use_example}}

Before using a tool, ensure all required parameters are available. If any required parameters are missing, do not attempt to use the tool.

<available_tools>{{tool_information}}</available_tools>

{{/if}}
