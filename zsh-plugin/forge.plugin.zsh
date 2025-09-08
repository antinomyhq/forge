#!/usr/bin/env zsh

# Forge ZSH Plugin - ZLE Widget Version
# Converts '# abc' to '$FORGE_CMD <<< abc' using ZLE widgets

# Configuration: Change this variable to customize the forge command
FORGE_CMD="target/debug/forge"

# Helper function for shared transformation logic
function _forge_transform_buffer() {
    # Check if the line starts with '# '
    if [[ "$BUFFER" =~ '^# (.*)$' ]]; then
        # Save the original command to history
        local original_command="$BUFFER"
        print -s "$original_command"
        
        # Extract the text after '# '
        local input_text="${match[1]}"
        
        # Transform to $FORGE_CMD command
        BUFFER="$FORGE_CMD <<< '$input_text'"
        
        # Move cursor to end
        CURSOR=${#BUFFER}
        
        return 0  # Successfully transformed
    fi
    return 1  # No transformation needed
}

# ZLE widget to transform # commands
function forge-transform-hash() {
    # Attempt transformation using helper
    if _forge_transform_buffer; then
        # Optionally auto-execute (uncomment the next line to auto-execute)
        # zle accept-line
        return
    fi
    # No transformation occurred, do nothing
}

# ZLE widget for Enter key that checks for # prefix
function forge-accept-line() {
    # Attempt transformation using helper
    if _forge_transform_buffer; then
        # Execute the transformed command directly (bypass history for this)
        echo  # Add a newline before execution for better UX
        eval "$BUFFER"
        
        # Clear the buffer and reset prompt
        BUFFER=""
        CURSOR=0
        zle reset-prompt
        return
    fi
    
    # For non-# commands, use normal accept-line
    zle accept-line
}

# ZLE widget that triggers on space after #
function forge-expand-hash() {
    # Only trigger if we just typed a space and the buffer starts with '#'
    if [[ "$BUFFER" == "# " ]]; then
        # Don't transform yet, wait for more input
        zle self-insert
        return
    elif [[ "$BUFFER" =~ '^# .+$' ]]; then
        # We have content after '# ', insert the space normally
        zle self-insert
        return
    else
        # Normal space behavior
        zle self-insert
    fi
}

# Register ZLE widgets
zle -N forge-transform-hash
zle -N forge-accept-line
zle -N forge-expand-hash

# Bind the transform widget to Ctrl+T (you can change this)
bindkey '^T' forge-transform-hash

# Bind Enter to our custom accept-line that transforms # commands
bindkey '^M' forge-accept-line
bindkey '^J' forge-accept-line

# Optional: Bind space to handle # expansion
# bindkey ' ' forge-expand-hash

# Alternative approach: Auto-transform on Enter
# This version transforms and shows the command before execution

# Function for manual transformation (fallback)
function forge-hash() {
    if [[ $# -eq 0 ]]; then
        echo "Usage: forge-hash <text>"
        echo "Converts text to '$FORGE_CMD <<< <text>'"
        return 1
    fi
    
    local input="$*"
    echo "$FORGE_CMD <<< '$input'"
    $FORGE_CMD <<< "$input"
}