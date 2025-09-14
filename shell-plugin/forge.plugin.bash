#!/usr/bin/env bash

# Forge Bash Plugin - READLINE Version
# Converts '# abc' to '$FORGE_CMD <<< abc' using bash READLINE

# Configuration: Change these variables to customize the forge command and special characters
FORGE_CMD="${FORGE_CMD:-forge}"
FORGE_RESUME_CONV="#\?\?"
FORGE_NEW_CONV="#\?"

# Helper function for shared transformation logic
function _forge_transform_buffer() {
    local forge_cmd=""
    local input_text=""
    
    # Check if the line starts with resume character (default: '?? ')
    if [[ "$READLINE_LINE" =~ ^${FORGE_RESUME_CONV}\ (.*)$ ]]; then
        forge_cmd="$FORGE_CMD --resume"
        input_text="${BASH_REMATCH[1]}"
    # Check if the line starts with new conversation character (default: '? ')
    elif [[ "$READLINE_LINE" =~ ^${FORGE_NEW_CONV}\ (.*)$ ]]; then
        forge_cmd="$FORGE_CMD"
        input_text="${BASH_REMATCH[1]}"
    else
        return 1  # No transformation needed
    fi
    
    # Save the original command to history
    local original_command="$READLINE_LINE"
    history -s "$original_command"
    
    # Transform to forge command with proper quoting
    READLINE_LINE="$forge_cmd <<< $(printf %q "$input_text")"
    
    # Move cursor to end
    READLINE_POINT=${#READLINE_LINE}
    
    return 0  # Successfully transformed
}

# Function to handle Enter key press and check for # prefix
function forge_accept_line() {
    # Attempt transformation using helper
    if _forge_transform_buffer; then
        # Execute the transformed command directly
        echo  # Add a newline before execution for better UX
        eval "$READLINE_LINE"
        
        # Clear the buffer and reset prompt
        READLINE_LINE=""
        READLINE_POINT=0
        return 0
    fi
    
    # For non-# commands, use normal accept-line
    return 1
}

# Alternative approach using bind -x for direct command execution
function forge_bind_command() {
    # Check if we need to transform the current line
    if [[ "$READLINE_LINE" =~ ^${FORGE_RESUME_CONV}\ (.*)$ ]] || [[ "$READLINE_LINE" =~ ^${FORGE_NEW_CONV}\ (.*)$ ]]; then
        if _forge_transform_buffer; then
            # Execute the transformed command
            echo
            eval "$READLINE_LINE"
            
            # Clear the buffer
            READLINE_LINE=""
            READLINE_POINT=0
            return 0
        fi
    fi
    
    # If no transformation, just accept the line normally
    return 1
}

# Setup function to bind the forge functionality
function forge_setup() {
    # Bind Ctrl+M (Enter) and Ctrl+J to our custom function
    # We use a wrapper function that calls both our handler and the default accept
    bind -x '"\C-m": forge_bind_wrapper'
    bind -x '"\C-j": forge_bind_wrapper'
}

# Wrapper function that tries our handler first, then falls back to default
function forge_bind_wrapper() {
    if ! forge_bind_command; then
        # If our handler returned non-zero, use default accept-line
        # In bash, we need to simulate this by calling the default behavior
        # Since we can't directly call accept-line in bind -x, we'll use a different approach
        # We'll let the default behavior happen by not modifying READLINE_LINE
        # and letting bash handle it naturally
        :
    fi
}

# Alternative setup using PROMPT_COMMAND approach for broader compatibility
function forge_prompt_command_setup() {
    # This approach checks for the pattern before each prompt
    # and transforms if needed, then executes immediately
    if [[ -n "$FORGE_PENDING_COMMAND" ]]; then
        eval "$FORGE_PENDING_COMMAND"
        unset FORGE_PENDING_COMMAND
        return
    fi
    
    # Check if the last command matches our pattern
    local last_command
    last_command=$(history 1 | sed 's/^[ ]*[0-9]*[ ]*//')
    
    if [[ "$last_command" =~ ^${FORGE_RESUME_CONV}\ (.*)$ ]]; then
        FORGE_PENDING_COMMAND="$FORGE_CMD --resume <<< $(printf %q "${BASH_REMATCH[1]}")"
        # Clear the current line and redisplay
        READLINE_LINE=""
        READLINE_POINT=0
    elif [[ "$last_command" =~ ^${FORGE_NEW_CONV}\ (.*)$ ]]; then
        FORGE_PENDING_COMMAND="$FORGE_CMD <<< $(printf %q "${BASH_REMATCH[1]}")"
        # Clear the current line and redisplay
        READLINE_LINE=""
        READLINE_POINT=0
    fi
}

# Auto-setup when the script is sourced
if [[ $- == *i* ]]; then
    # Only setup in interactive shells
    
    # Try the bind -x approach first (more responsive)
    if forge_setup 2>/dev/null; then
        # Successfully loaded with bind-x support (silent)
        :
    else
        # Fallback to PROMPT_COMMAND approach
        if [[ -n "$PROMPT_COMMAND" ]]; then
            PROMPT_COMMAND="$PROMPT_COMMAND; forge_prompt_command_setup"
        else
            PROMPT_COMMAND="forge_prompt_command_setup"
        fi
        # Successfully loaded with PROMPT_COMMAND fallback (silent)
        :
    fi
fi