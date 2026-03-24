import os
import re

path = 'shell-plugin/lib/dispatcher.zsh'
with open(path, 'r') as f:
    content = f.read()

# Add a hook to capture the last command output from the zsh history
# We'll inject this into the _forge_exec_interactive call
new_dispatch = """    # Capture last command from history for context
    local last_cmd=$(fc -ln -1)
    
    # Execute the forge command with last command context
    _forge_exec_interactive -p "$input_text" --cid "$_FORGE_CONVERSATION_ID" --context "Last command: $last_cmd"
"""

content = content.replace('_forge_exec_interactive -p "$input_text" --cid "$_FORGE_CONVERSATION_ID"', new_dispatch)

with open(path, 'w') as f:
    f.write(content)
