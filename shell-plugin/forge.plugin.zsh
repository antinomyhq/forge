#!/usr/bin/env zsh

# Forge ZSH Plugin - ZLE Widget Version  
# Converts agent-tagged commands to resume conversations using ZLE widgets
# Supports :plan/:p (muse), :ask/:a (sage), :agent_name (custom agent), : (forge default)
# Features: Auto-resume existing conversations or start new ones, @ tab completion support

# Configuration: Change these variables to customize the forge command and special characters
# Using typeset to keep variables local to plugin scope and prevent public exposure
typeset -h _FORGE_BIN="${FORGE_BIN:-forge}"
typeset -h _FORGE_CONVERSATION_PATTERN=":"

ZSH_HIGHLIGHT_HIGHLIGHTERS+=(pattern)
# Style the conversation pattern with appropriate highlighting
# Keywords in yellow, rest in default white

# Highlight colon + word at the beginning in yellow
ZSH_HIGHLIGHT_PATTERNS+=('(#s):[a-zA-Z]#' 'fg=yellow,bold')

# Highlight everything after that word + space in white
ZSH_HIGHLIGHT_PATTERNS+=('(#s):[a-zA-Z]# *(*|[[:graph:]]*)' 'fg=white,bold')



# Store conversation ID in a temporary variable (local to plugin)
typeset -h _FORGE_CONVERSATION_ID=""

# Helper function for shared transformation logic
function _forge_transform_buffer() {
    local forge_cmd=""
    local input_text=""
    local agent=""
    
    # Check if the line starts with any of the supported patterns
    if [[ "$BUFFER" =~ "^:plan (.*)$" ]] || [[ "$BUFFER" =~ "^:p (.*)$" ]]; then
        input_text="${match[1]}"
        agent="muse"
    elif [[ "$BUFFER" =~ "^:ask (.*)$" ]] || [[ "$BUFFER" =~ "^:a (.*)$" ]]; then
        input_text="${match[1]}"
        agent="sage"
    elif [[ "$BUFFER" =~ "^:([a-zA-Z][a-zA-Z0-9_-]*) (.*)$" ]]; then
        agent="${match[1]}"
        input_text="${match[2]}"
    elif [[ "$BUFFER" =~ "^: (.*)$" ]]; then
        input_text="${match[1]}"
        agent="forge"  # Default agent
    else
        return 1  # No transformation needed
    fi
    
    # Always try to resume - if no conversation ID exists, generate a new one
    if [[ -z "$_FORGE_CONVERSATION_ID" ]]; then
        _FORGE_CONVERSATION_ID=$($_FORGE_BIN --generate-conversation-id)
    fi
    
    # Build the forge command with the appropriate agent
    forge_cmd="$_FORGE_BIN --resume $_FORGE_CONVERSATION_ID --agent $agent"
    
    # Save the original command to history
    local original_command="$BUFFER"
    print -s "$original_command"
    
    # Transform to forge command
    BUFFER="$forge_cmd <<< $(printf %q "$input_text")"
    
    # Move cursor to end
    CURSOR=${#BUFFER}
    
    return 0  # Successfully transformed
}

# ZLE widget for @ tab completion - opens fd | fzf
function forge-at-completion() {
    local current_word="${LBUFFER##* }"
    
    # Check if the current word starts with @
    if [[ "$current_word" =~ ^@.*$ ]]; then
        # Extract the part after @ for filtering
        local filter_text="${current_word#@}"
        
        # Use fd to find files and fzf for interactive selection
        local selected
        if [[ -n "$filter_text" ]]; then
            # If there's text after @, use it as initial filter
            selected=$(fd --type f --hidden --exclude .git | fzf --query "$filter_text" --height 40% --reverse)
        else
            # If just @ was typed, show all files
            selected=$(fd --type f --hidden --exclude .git | fzf --height 40% --reverse)
        fi
        
        # If a file was selected, replace the @ text with the selected file path
        if [[ -n "$selected" ]]; then
            selected="@[${selected}]"
            # Remove the @ and any text after it from LBUFFER
            LBUFFER="${LBUFFER%$current_word}"
            # Add the selected file path
            BUFFER="${LBUFFER}${selected}${RBUFFER}"
            # Move cursor to end of inserted text
            CURSOR=$((${#LBUFFER} + ${#selected}))
        fi
        
        # Reset the prompt
        zle reset-prompt
        return 0
    fi
    
    # If not @ completion, fall back to default completion
    zle expand-or-complete
}

function forge-accept-line() {
    # Attempt transformation using helper
    if _forge_transform_buffer; then
        # Execute the transformed command directly (bypass history for this)
        echo  # Add a newline before execution for better UX
        eval "$BUFFER"
        
        # Set buffer to conversation pattern for continued interaction
        BUFFER="${_FORGE_CONVERSATION_PATTERN} "
        CURSOR=${#BUFFER}
        zle reset-prompt
        return
    fi
    
    # For non-:commands, use normal accept-line
    zle accept-line
}

# Register ZLE widgets
zle -N forge-accept-line
zle -N forge-at-completion

# Bind Enter to our custom accept-line that transforms :commands
bindkey '^M' forge-accept-line
bindkey '^J' forge-accept-line

# Bind Tab to our custom @ completion widget  
bindkey '^I' forge-at-completion  # Tab for @ completion