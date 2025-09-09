#!/usr/bin/env zsh

# Forge ZSH Plugin - ZLE Widget Version
# Converts '# abc' to '$FORGE_CMD <<< abc' using ZLE widgets

# Configuration: Change this variable to customize the forge command
FORGE_CMD="forge"

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
        BUFFER="$FORGE_CMD <<< $(printf %q "$input_text")"
        
        # Move cursor to end
        CURSOR=${#BUFFER}
        
        return 0  # Successfully transformed
    fi
    return 1  # No transformation needed
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

# Register ZLE widgets
zle -N forge-accept-line

# Bind Enter to our custom accept-line that transforms # commands
bindkey '^M' forge-accept-line
bindkey '^J' forge-accept-line

