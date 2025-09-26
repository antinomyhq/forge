#!/usr/bin/env zsh

# Forge ZSH Plugin - ZLE Widget Version  
# Converts command-tagged commands to resume conversations using ZLE widgets
# Supports :plan/:p (muse), :ask/:a (sage), :new (start new conversation), :command_name (custom command), : (forge default)
# Features: Auto-resume existing conversations or start new ones, @ tab completion support, banner display for new conversations

# Configuration: Change these variables to customize the forge command and special characters
# Using typeset to keep variables local to plugin scope and prevent public exposure
typeset -h _FORGE_BIN="${FORGE_BIN:-forge}"
typeset -h _FORGE_CONVERSATION_PATTERN=":"
typeset -h _FORGE_RESET_COMMAND="reset"

# Style tagged files to be in green
ZSH_HIGHLIGHT_PATTERNS+=('@\[[^]]#\]' 'fg=green,bold')

ZSH_HIGHLIGHT_HIGHLIGHTERS+=(pattern)
# Style the conversation pattern with appropriate highlighting
# Keywords in yellow, rest in default white

# Highlight colon + word at the beginning in yellow
ZSH_HIGHLIGHT_PATTERNS+=('(#s):[a-zA-Z]#' 'fg=yellow,bold')

# Highlight everything after that word + space in white
ZSH_HIGHLIGHT_PATTERNS+=('(#s):[a-zA-Z]# *(*|[[:graph:]]*)' 'fg=white,bold')



# Store conversation ID in a temporary variable (local to plugin)
typeset -h _FORGE_CONVERSATION_ID=""

# Store the last command for reuse
typeset -h _FORGE_USER_ACTION=""


# Helper function for shared transformation logic
function _forge_transform_buffer() {
    local forge_cmd=""
    local input_text=""
    
    # Check if the line starts with any of the supported patterns
    if [[ "$BUFFER" =~ "^:([a-zA-Z][a-zA-Z0-9_-]*)( (.*))?$" ]]; then
        # Action with or without parameters: :foo or :foo bar baz
        _FORGE_USER_ACTION="${match[1]}"
        input_text="${match[3]:-}"  # Use empty string if no parameters (match[2] is the space + params, match[3] is just params)
    elif [[ "$BUFFER" =~ "^: (.*)$" ]]; then
        # Default action with parameters: : something
        input_text="${match[1]}"        
    else
        return 1  # No transformation needed
    fi

    # Handle `new` as a special case
    if [[ "$_FORGE_USER_ACTION" == "$_FORGE_RESET_COMMAND" ]]; then
        return 1 # No transformation needed
    fi
        
    # Always try to resume - if no conversation ID exists, generate a new one
    if [[ -z "$_FORGE_CONVERSATION_ID" ]]; then
        _FORGE_CONVERSATION_ID=$($_FORGE_BIN --generate-conversation-id)
    fi
    
    # Build the forge command with the appropriate command
    forge_cmd="$_FORGE_BIN --resume $_FORGE_CONVERSATION_ID --agent ${_FORGE_USER_ACTION:-forge}"        
    
    # Return the transformed command without modifying BUFFER
    echo "$forge_cmd -p $(printf %q "$input_text")"
    
    return 0  # Successfully transformed
}

# Custom completion widget that handles both :commands and @ completion
function forge-completion() {
    local current_word="${LBUFFER##* }"
    
    # Handle @ completion (existing functionality)
    if [[ "$current_word" =~ ^@.*$ ]]; then
        local filter_text="${current_word#@}"
        local selected
        if [[ -n "$filter_text" ]]; then
            selected=$(fd --type f --hidden --exclude .git | fzf --select-1 --query "$filter_text" --height 40% --reverse)
        else
            selected=$(fd --type f --hidden --exclude .git | fzf --select-1 --height 40% --reverse)
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
                selected=$(echo "$command_output" | fzf --select-1 --nth=1 --query "$filter_text" --height 40% --reverse --prompt="Agent ❯ ")
            else
                selected=$(echo "$command_output" | fzf --select-1 --nth=1 --height 40% --reverse --prompt="Agent ❯ ")
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
    # Attempt transformation using helper
    local transformed_command
    if transformed_command=$(_forge_transform_buffer); then
        # Execute the transformed command directly (bypass history for this)
        echo  # Add a newline before execution for better UX
        eval "$transformed_command"
        
        # Only update buffer after successful execution
        BUFFER="${_FORGE_CONVERSATION_PATTERN}${_FORGE_RESET_COMMAND}"
        CURSOR=${#BUFFER}
        zle reset-prompt
        return
    fi

    if [[ "$_FORGE_USER_ACTION" == "$_FORGE_RESET_COMMAND" ]]; then
        echo
        if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
            echo "\033[36m⏺\033[0m \033[90m[$(date '+%H:%M:%S')] Reset ${_FORGE_CONVERSATION_ID}\033[0m"
        fi
        
        _FORGE_CONVERSATION_ID=""
        unset _FORGE_USER_ACTION
        BUFFER=""
        CURSOR=${#BUFFER}
        zle reset-prompt
        return 0
    fi
    
    # For non-:commands, use normal accept-line
    zle accept-line
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