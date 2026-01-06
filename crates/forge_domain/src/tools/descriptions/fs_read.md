Reads a file from the local filesystem. You can access any file directly by using this tool.
Assume this tool is able to read all non binary files on the machine. Binary files are automatically detected and rejected. If the User provides a path to a file assume that path is valid. It is okay to read a file that does not exist; an error will be returned.

Usage:
- The path parameter must be an absolute path, not a relative path
- By default, it reads up to {{env.maxReadSize}} lines starting from the beginning of the file
- You can optionally specify a line start_line and end_line (especially handy for long files), but it's recommended to read the whole file by not providing these parameters
- Any files larger than {{env.maxFileSize}} bytes will return error
- Results are returned using rg "" -n format, with line numbers starting at 1
- This tool can only read non binary files, not directories. To read a directory, use an ls command via the `shell` tool. To read an image use the `read_image` tool.
- You can call multiple tools in a single response. It is always better to speculatively read multiple potentially useful files in parallel.