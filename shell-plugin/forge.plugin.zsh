#!/usr/bin/env zsh

# Documentation in [README.md](./README.md)


# Configuration: Change these variables to customize the forge command and special characters
# Using typeset to keep variables local to plugin scope and prevent public exposure
typeset -h _FORGE_BIN="${FORGE_BIN:-forge}"
typeset -h _FORGE_CONVERSATION_PATTERN=":"

# Style tagged files
ZSH_HIGHLIGHT_PATTERNS+=('@\[[^]]#\]' 'fg=cyan,bold')

ZSH_HIGHLIGHT_HIGHLIGHTERS+=(pattern)
# Style the conversation pattern with appropriate highlighting
# Keywords in yellow, rest in default white

# Highlight colon + word at the beginning in yellow
ZSH_HIGHLIGHT_PATTERNS+=('(#s):[a-zA-Z]#' 'fg=yellow,bold')

# Highlight everything after that word + space in white
ZSH_HIGHLIGHT_PATTERNS+=('(#s):[a-zA-Z]# *(*|[[:graph:]]*)' 'fg=white,bold')

# Private fzf function with common options for consistent UX
function _forge_fzf() {
    fzf --cycle --select-1 --height 40% --reverse "$@"
}



# Store conversation ID in a temporary variable (local to plugin)
export FORGE_CONVERSATION_ID=""
export FORGE_ACTIVE_AGENT=""

# Store the last command for reuse
typeset -h _FORGE_USER_ACTION=""

# Custom completion widget that handles both :commands and @ completion
function forge-completion() {
    local current_word="${LBUFFER##* }"
    
    # Handle @ completion (existing functionality)
    if [[ "$current_word" =~ ^@.*$ ]]; then
        local filter_text="${current_word#@}"
        local selected
        if [[ -n "$filter_text" ]]; then
            selected=$(fd --type f --hidden --exclude .git | _forge_fzf --query "$filter_text")
        else
            selected=$(fd --type f --hidden --exclude .git | _forge_fzf)
        fi
        
        if [[ -n "$selected" ]]; then
            selected="@[${selected}]"
            LBUFFER="${LBUFFER%$current_word}"
            BUFFER="${LBUFFER}${selected}${RBUFFER}"
            CURSOR=$((${#LBUFFER} + ${#selected}))
        fi
        
        zle reset-prompt
        return 0
    fi
    
    # Handle :command completion
    if [[ "${LBUFFER}" =~ "^:[a-zA-Z]*$" ]]; then
        # Extract the text after the colon for filtering
        local filter_text="${LBUFFER#:}"
        
        # Get available commands from forge show-agents
        local command_output
        command_output=$($_FORGE_BIN show-agents 2>/dev/null)
        
        if [[ $? -eq 0 && -n "$command_output" ]]; then
            # Use fzf for interactive selection with prefilled filter
            local selected
            if [[ -n "$filter_text" ]]; then
                selected=$(echo "$command_output" | _forge_fzf --nth=1 --query "$filter_text" --prompt="Agent ❯ ")
            else
                selected=$(echo "$command_output" | _forge_fzf --nth=1 --prompt="Agent ❯ ")
            fi
            
            if [[ -n "$selected" ]]; then
                # Extract just the command name (first word before any description)
                local command_name="${selected%% *}"
                # Replace the current buffer with the selected command
                BUFFER=":$command_name "
                CURSOR=${#BUFFER}
            fi
        fi
        
        zle reset-prompt
        return 0
    fi
    
    # Fall back to default completion
    zle expand-or-complete
}

function forge-accept-line() {
    # Save the original command for history
    local original_buffer="$BUFFER"
    
    # Parse the buffer first in parent shell context to avoid subshell issues
    local user_action=""
    local input_text=""
    
    # Check if the line starts with any of the supported patterns
    if [[ "$BUFFER" =~ "^:([a-zA-Z][a-zA-Z0-9_-]*)( (.*))?$" ]]; then
        # Action with or without parameters: :foo or :foo bar baz
        user_action="${match[1]}"
        input_text="${match[3]:-}"  # Use empty string if no parameters
        elif [[ "$BUFFER" =~ "^: (.*)$" ]]; then
        # Default action with parameters: : something
        user_action=""
        input_text="${match[1]}"
    else
        # For non-:commands, use normal accept-line
        zle accept-line
        return
    fi
    
    # Add the original command to history before transformation
    print -s -- "$original_buffer"
    
    # Handle reset command specially
    if [[ "$user_action" == "reset" || "$user_action" == "r" ]]; then
        echo
        if [[ -n "$FORGE_CONVERSATION_ID" ]]; then
            echo "\033[36m⏺\033[0m \033[90m[$(date '+%H:%M:%S')] Reset ${FORGE_CONVERSATION_ID}\033[0m"
        fi
        
        FORGE_CONVERSATION_ID=""
        _FORGE_USER_ACTION=""
        BUFFER=""
        CURSOR=${#BUFFER}
        zle reset-prompt
        return 0
    fi
    
    # Store the action for potential reuse
    _FORGE_USER_ACTION="$user_action"
    
    # Generate conversation ID if needed (in parent shell context)
    if [[ -z "$FORGE_CONVERSATION_ID" ]]; then
        FORGE_CONVERSATION_ID=$($_FORGE_BIN --generate-conversation-id)
    fi
    
    # Set the active agent for this execution
    FORGE_ACTIVE_AGENT="${user_action:-${FORGE_ACTIVE_AGENT:-forge}}"
    
    # Build and execute the forge command
    local forge_cmd="$_FORGE_BIN"
    local quoted_input=${input_text//\'/\'\\\'\'}
    local full_command="$forge_cmd -p '$quoted_input'"
    
    # Set buffer to the transformed command and execute
    BUFFER="$full_command"
    zle accept-line
    return
}

# Register ZLE widgets
zle -N forge-accept-line
# Register completions
zle -N forge-completion


# Bind Enter to our custom accept-line that transforms :commands
bindkey '^M' forge-accept-line
bindkey '^J' forge-accept-line
# Update the Tab binding to use the new completion widget
bindkey '^I' forge-completion  # Tab for both @ and :command completion