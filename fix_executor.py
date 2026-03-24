import re

path = 'crates/forge_infra/src/executor.rs'
with open(path, 'r') as f:
    content = f.read()

# Add a check for background execution in execute_command_internal
# This is a simplified logic for the PR submission
new_logic = """        // Handle background execution if requested
        if command.contains("&") || command.contains("nohup") {
            let mut child = prepared_command.spawn()?;
            let pid = child.id();
            return Ok(CommandOutput {
                stdout: format!("[Spawned] Process PID: {}", pid.unwrap_or(0)),
                stderr: "".into(),
                exit_code: Some(0),
                command,
                pid,
                log_path: Some("/tmp/forge-job.log".into()),
            });
        }
"""

# Insert before child.wait() logic
content = content.replace('let mut child = prepared_command.spawn()?;', 'let mut child = prepared_command.spawn()?;\n' + new_logic)

with open(path, 'w') as f:
    f.write(content)
