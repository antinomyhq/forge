1. You can only make one tool call per message.
2. Each tool call must be wrapped in `<forge_tool_call>` tags.
3. The tool call must be in JSON format with the following structure:
    - The `name` field must specify the tool name.
    - The `arguments` field must contain the required parameters for the tool.

Here's a correct example structure:

Example 1:
<forge_tool_call>
{"name": "read", "arguments": {"path": "/a/b/c.txt"}}
</forge_tool_call>

Example 2:
<forge_tool_call>
{"name": "write", "arguments": {"path": "/a/b/c.txt", "content": "Hello World!"}}
</forge_tool_call>

Important:
1. ALWAYS use JSON format inside `forge_tool_call` tags.
2. Specify the name of tool in the `name` field.
3. Specify the tool arguments in the `arguments` field.
4. If you need to make multiple tool calls, send them in separate messages.

Before using a tool, ensure all required arguments are available. 
If any required arguments are missing, do not attempt to use the tool.
