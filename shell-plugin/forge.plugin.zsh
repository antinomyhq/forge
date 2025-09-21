#!/usr/bin/env zsh

# Forge ZSH Plugin - ZLE Widget Version
# Converts '?? abc' to always resume conversations using ZLE widgets
# Features: Auto-resume existing conversations or start new ones, @ tab completion support

# Configuration: Change these variables to customize the forge command and special characters
# Using typeset to keep variables local to plugin scope and prevent public exposure
typeset -h _FORGE_BIN="${FORGE_BIN:-forge}"
typeset -h _FORGE_CONVERSATION_PATTERN="\?\?"

ZSH_HIGHLIGHT_HIGHLIGHTERS+=(pattern)
# Style the conversation pattern with appropriate highlighting
ZSH_HIGHLIGHT_PATTERNS+=('(#s)\?\? *' 'fg=white,bold')


# Store conversation ID in a temporary variable (local to plugin)
typeset -h _FORGE_CONVERSATION_ID=""

# Helper function for shared transformation logic
function _forge_transform_buffer() {
    local forge_cmd=""
    local input_text=""
    
    # Check if the line starts with the conversation pattern (default: '??')
    if [[ "$BUFFER" =~ "^${_FORGE_CONVERSATION_PATTERN}(.*)$" ]]; then
        input_text="${match[1]}"
        
        # Always try to resume - if no conversation ID exists, generate a new one
        if [[ -z "$_FORGE_CONVERSATION_ID" ]]; then
            _FORGE_CONVERSATION_ID=$($_FORGE_BIN --generate-conversation-id)
        fi
        
        forge_cmd="$_FORGE_BIN --resume $_FORGE_CONVERSATION_ID"
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



# ZLE widget for Enter key that transforms #? commands to always resume conversations
# ZLE widget for inserting conversation pattern
function forge-insert-pattern() {
    # Toggle the conversation pattern at the beginning of the line
    # while maintaining cursor position relative to the original text
    local pattern="?? "
    local original_cursor_pos=$CURSOR
    
    # Check if buffer already starts with the pattern
    if [[ "$BUFFER" =~ "^${_FORGE_CONVERSATION_PATTERN} " ]]; then
        # Remove pattern from the beginning
        BUFFER="${BUFFER#${pattern}}"
        
        # Adjust cursor position, but don't go below 0
        CURSOR=$((original_cursor_pos - ${#pattern}))
        if [[ $CURSOR -lt 0 ]]; then
            CURSOR=0
        fi
    else
        # Insert pattern at the beginning of the buffer
        BUFFER="${pattern}${BUFFER}"
        
        # Adjust cursor position to account for the inserted pattern length
        CURSOR=$((original_cursor_pos + ${#pattern}))
    fi
    
    zle redisplay
}
# Function to clear the current conversation ID
function forge-clear-conversation() {
    _FORGE_CONVERSATION_ID=""    
}

# ZLE widget that triggers file picker immediately when @ is typed
function forge-at-input() {
    # Insert the @ character first
    LBUFFER="${LBUFFER}@"
    
    # Immediately trigger file selection
    local selected
    selected=$(fd --type f --hidden --exclude .git | fzf --height 40% --reverse --prompt "Select file: ")
    
    # If a file was selected, replace the @ with the selected file path
    if [[ -n "$selected" ]]; then
        # Remove the @ we just added
        LBUFFER="${LBUFFER%@}"
        # Add the selected file path in the proper format
        selected="@[${selected}]"
        LBUFFER="${LBUFFER}${selected}"
        CURSOR=${#LBUFFER}
    fi
    
    # Reset the prompt
    zle reset-prompt
}

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
    
    # For non-?? commands, use normal accept-line
    zle accept-line
}

# Register ZLE widgets
zle -N forge-insert-pattern
zle -N forge-accept-line
zle -N forge-clear-conversation
zle -N forge-at-input

# Bind Enter to our custom accept-line that transforms ?? commands
bindkey '^M' forge-accept-line
bindkey '^J' forge-accept-line

# Bind CTRL+G to insert/toggle conversation pattern  
bindkey '^G' forge-insert-pattern

# Bind CTRL+K to clear conversation
bindkey '^K' forge-clear-conversation
# Bind @ character to trigger immediate file picker
bindkey '@' forge-at-input