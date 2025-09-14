#!/usr/bin/env zsh

# Forge ZSH Plugin - ZLE Widget Version
# Converts '# abc' to '$FORGE_CMD <<< abc' using ZLE widgets

# Configuration: Change these variables to customize the forge command and special characters
FORGE_CMD="${FORGE_CMD:-forge}"
FORGE_RESUME_CONV="#\?\?"
FORGE_NEW_CONV="#\?"

# Helper function for shared transformation logic
function _forge_transform_buffer() {
    local forge_cmd=""
    local input_text=""
    
    # Check if the line starts with resume character (default: '?? ')
    if [[ "$BUFFER" =~ "^${FORGE_RESUME_CONV} (.*)$" ]]; then
        forge_cmd="$FORGE_CMD --resume"
        input_text="${match[1]}"
    # Check if the line starts with new conversation character (default: '? ')
    elif [[ "$BUFFER" =~ "^${FORGE_NEW_CONV} (.*)$" ]]; then
        forge_cmd="$FORGE_CMD"
        input_text="${match[1]}"
    else
        return 1  # No transformation needed
    fi
    
    # Save the original command to history
    local original_command="$BUFFER"
    print -s "$original_command"
    
    # Transform to forge command
    BUFFER="$forge_cmd <<< $(printf %q "$input_text")"
    
    # Move cursor to end
    CURSOR=${#BUFFER}
    
    return 0  # Successfully transformed
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