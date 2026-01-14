Performs exact string replacements in files.

Required parameters: `file_path`, `old_string`, `operation`, `new_string`

Usage:
- You must use your `{{tool_names.read}}` tool at least once in the conversation before editing. This tool will error if you attempt an edit without reading the file. 
- When editing text from `{{tool_names.read}}` tool output, ensure you preserve the exact indentation (tabs/spaces) as it appears AFTER the line number prefix. The line number prefix format is: 'line_number:'. Everything after that line_number: is the actual file content to match. Never include any part of the line number prefix in the old_string or new_string.
- ALWAYS prefer editing existing files in the codebase. NEVER write new files unless explicitly required.
- Only use emojis if the user explicitly requests it. Avoid adding emojis to files unless asked.
- The edit will FAIL if `old_string` is not unique in the file. Either provide a larger string with more surrounding context to make it unique or use `replace_all` to change every instance of `old_string`. 
- Use `replace_all` for replacing and renaming strings across the file. This parameter is useful if you want to rename a variable for instance.