Writes data to the standard input (stdin) of an interactive process session and returns any new output.

This tool manages interactive process sessions (REPLs, database CLIs, game engines, etc.) that read from stdin. On the first call for a given session_id, provide `shell_command` to spawn the process. Subsequent calls to the same session_id reuse the existing session.

Usage:
  - First call: provide `session_id` (any string you choose), `shell_command` (the command to run), and `input` (first input to send).
  - Subsequent calls: provide the same `session_id` and `input`. Omit `shell_command` (it is ignored after the session is created).
  - The `input` string is written directly to the process's stdin pipe, followed by a newline.
  - Use `command` to provide a short human-readable description of what the input does.
  - The tool returns stdout and stderr output captured within a 5-second window after writing.

Examples:
  - Start a Python REPL: session_id: "py", shell_command: "python3 -i", input: "print('hello')", command: "Start Python and print hello"
  - Send more input: session_id: "py", input: "2 + 2", command: "Calculate sum"
  - Start a database CLI: session_id: "db", shell_command: "sqlite3 :memory:", input: "CREATE TABLE t(id INTEGER);", command: "Create table"

Notes:
  - The process must still be running; writing to a dead process returns is_alive=false.
  - If the session_id does not exist and no shell_command is provided, an error is returned.
  - Output is captured with a 5-second timeout after each write. If the process produces output slowly, make another call with empty input to read more.
