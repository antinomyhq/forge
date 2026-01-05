Executes shell commands.
The `cwd` parameter sets the working directory for command execution.
CRITICAL: Do NOT use `cd` commands in the command string. This is FORBIDDEN.
Always use the `cwd` parameter to set the working directory instead. Any use
of `cd` in the command is redundant, incorrect, and violates the tool
contract. Use for running utilities, installing packages, or executing
build commands. Returns complete output including
stdout, stderr, and exit code for diagnostic purposes.
