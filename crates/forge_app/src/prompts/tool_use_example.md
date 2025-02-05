Here's a correct example structure:

```xml
<tool_call>
<tool_forge_fs_read>
<path>/path/to/file</path>
<recursive>true</recursive>
</tool_forge_fs_read>
</tool_call>
```

Example of correct multi-step tool usage:

First message:
<tool_call>
<tool_forge_fs_read>
<path>/path/to/file</path>
</tool_forge_fs_read>
</tool_call>

After receiving response, second message:
<tool_call>
<tool_forge_fs_create>
<path>/path/to/file</path>
<content>New content</content>
</tool_forge_fs_create>
</tool_call>
