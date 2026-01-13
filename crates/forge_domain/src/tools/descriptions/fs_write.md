Writes a file to the local filesystem.

Usage:
- The path parameter must be an absolute path, not a relative path
- The tool automatically handles the creation of any missing intermediary directories in the specified path
- For existing files being overwritten (when overwrite=true), snapshots are automatically created to enable undo functionality.
- If a file exists and 'overwrite' is false (or not set), an error will be returned indicating the file already exists
- Files are automatically validated for syntax errors when possible; validation failures are reported but don't prevent file creation
- Snapshots are automatically created before overwriting existing files to enable undo functionality
- IMPORTANT: DO NOT attempt to use this tool to move or rename files, use the shell tool instead.
- ALWAYS prefer editing existing files in the codebase. NEVER write new files unless explicitly required.
- NEVER proactively create documentation files (*.md) or README files. Only create documentation files if explicitly requested by the User.
- Only use emojis if the user explicitly requests it. Avoid writing emojis to files unless asked.
