Writes a file to the local filesystem.

Usage:
- This tool will overwrite the existing file if there is one at the provided path.
- If this is an existing file, you MUST use the {{tool_names.read}} tool first to read the file's contents and use this tool with 'overwrite' as true . This tool will fail if you did not read the file first or don't set overwrite parameter to true.
- ALWAYS prefer {{tool_names.patch}} on existing files in the codebase. NEVER write new files unless explicitly required.
- NEVER proactively create documentation files (*.md) or README files. Only create documentation files if explicitly requested by the User.
- Only use emojis if the user explicitly requests it. Avoid writing emojis to files unless asked.