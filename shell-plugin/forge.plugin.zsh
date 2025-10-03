#!/usr/bin/env zsh

# Documentation in [README.md](./README.md)


# Configuration: Change these variables to customize the forge command and special characters
# Using typeset to keep variables local to plugin scope and prevent public exposure
typeset -h _FORGE_BIN="${FORGE_BIN:-cargo run --quiet --}"
typeset -h _FORGE_CONVERSATION_PATTERN=":"

# Detect fd command - Ubuntu/Debian use 'fdfind', others use 'fd'
typeset -h _FORGE_FD_CMD="$(command -v fdfind 2>/dev/null || command -v fd 2>/dev/null || echo 'fd')"

# Cache the commands list once at plugin load time
typeset -h _FORGE_COMMANDS="$($_FORGE_BIN show-commands 2>/dev/null)"

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

# Helper function to print operating agent messages with consistent formatting
function _forge_print_agent_message() {
    local agent_name="${1:-${FORGE_ACTIVE_AGENT}}"
    echo "\033[33m⏺\033[0m \033[90m[$(date '+%H:%M:%S')] \033[1;37m${agent_name:u}\033[0m \033[90mis now the active agent\033[0m"
}

# Helper function to select and set config values with fzf
function _forge_select_and_set_config() {
    local show_command="$1"
    local config_flag="$2"
    local prompt_text="$3"
    
    (
        echo
        local output
        output=$($_FORGE_BIN "$show_command" 2>/dev/null)
        
        if [[ -n "$output" ]]; then
            local selected
            selected=$(echo "$output" | _forge_fzf --prompt="$prompt_text ❯ ")
            
            if [[ -n "$selected" ]]; then
                local name="${selected%% *}"
                $_FORGE_BIN config set "--$config_flag" "$name"
            fi
        fi
    )
}


# Helper function to handle session commands that require an active conversation
function _forge_handle_session_command() {
    local subcommand="$1"
    shift  # Remove first argument, remaining args become extra parameters
    
    echo
    
    # Check if FORGE_CONVERSATION_ID is set
    if [[ -z "$FORGE_CONVERSATION_ID" ]]; then
        echo "\033[31m✗\033[0m No active conversation. Start a conversation first or use :list to see existing ones"
        BUFFER=""
        CURSOR=${#BUFFER}
        zle reset-prompt
        return 0
    fi
    
    # Execute the session command with conversation ID and any extra arguments
    $_FORGE_BIN session --id "$FORGE_CONVERSATION_ID" "$subcommand" "$@"
    
    BUFFER=""
    CURSOR=${#BUFFER}
    zle reset-prompt
    return 0
}

# Store conversation ID in a temporary variable (local to plugin)
export FORGE_CONVERSATION_ID=""
export FORGE_ACTIVE_AGENT="forge"

# Custom completion widget that handles both :commands and @ completion
function forge-completion() {
    local current_word="${LBUFFER##* }"
    
    # Handle @ completion (existing functionality)
    if [[ "$current_word" =~ ^@.*$ ]]; then
        local filter_text="${current_word#@}"
        local selected
        if [[ -n "$filter_text" ]]; then
            selected=$($_FORGE_FD_CMD --type f --hidden --exclude .git | _forge_fzf --query "$filter_text")
        else
            selected=$($_FORGE_FD_CMD --type f --hidden --exclude .git | _forge_fzf)
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
        
        # Use the cached commands list
        if [[ -n "$_FORGE_COMMANDS" ]]; then
            # Use fzf for interactive selection with prefilled filter
            local selected
            if [[ -n "$filter_text" ]]; then
                selected=$(echo "$_FORGE_COMMANDS" | _forge_fzf --nth=1 --query "$filter_text" --prompt="Command ❯ ")
            else
                selected=$(echo "$_FORGE_COMMANDS" | _forge_fzf --nth=1 --prompt="Command ❯ ")
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
    

    # Handle aliases - convert to their actual agent names
    if [[ "$user_action" == "ask" ]]; then
        user_action="sage"
    elif [[ "$user_action" == "plan" ]]; then
        user_action="muse"
    fi
    
    # Handle new command specially
    if [[ "$user_action" == "new" || "$user_action" == "n" ]]; then
        echo
        # Show banner
        $_FORGE_BIN show-banner
        
        _forge_print_agent_message "FORGE"
        
        FORGE_CONVERSATION_ID=""
        FORGE_ACTIVE_AGENT="forge"
        BUFFER=""
        CURSOR=${#BUFFER}
        zle reset-prompt
        return 0
    fi
    
    # Handle info command specially
    if [[ "$user_action" == "info" || "$user_action" == "i" ]]; then
        echo
        
        # Run forge info
        $_FORGE_BIN info
        
        BUFFER=""
        CURSOR=${#BUFFER}
        zle reset-prompt
        return 0
    fi

    
    # Handle dump command specially  
    if [[ "$user_action" == "dump" ]]; then
        # Pass "html" as extra argument if specified, otherwise pass nothing
        if [[ "$input_text" == "html" ]]; then
            _forge_handle_session_command "dump" "html"
        else
            _forge_handle_session_command "dump"
        fi
        return 0
    fi
    
    # Handle compact command specially
    if [[ "$user_action" == "compact" ]]; then
        _forge_handle_session_command "compact"
        return 0
    fi
    
    # Handle retry command specially
    if [[ "$user_action" == "retry" ]]; then
        _forge_handle_session_command "retry"
        return 0
    fi
    
    # Handle list/conversations command specially
    if [[ "$user_action" == "conversation" ]]; then
        echo
        
        # Get conversations list
        local conversations_output
        conversations_output=$($_FORGE_BIN session --list 2>/dev/null)
        
        if [[ -n "$conversations_output" ]]; then
            # Get current conversation ID if set
            local current_id="$FORGE_CONVERSATION_ID"
            
            # Create prompt with current conversation
            local prompt_text="Conversation ❯ "
            if [[ -n "$current_id" ]]; then
                prompt_text="Conversation [Current: ${current_id}] ❯ "
            fi
            
            local selected_conversation
            selected_conversation=$(echo "$conversations_output" | _forge_fzf --prompt="$prompt_text")
            
            if [[ -n "$selected_conversation" ]]; then
                # Strip ANSI codes first, then extract the last field (UUID)
                local conversation_id=$(echo "$selected_conversation" | sed 's/\x1b\[[0-9;]*m//g' | sed 's/\x1b\[K//g' | awk '{print $NF}' | tr -d '\n')
                
                # Set the selected conversation as active (in parent shell)
                FORGE_CONVERSATION_ID="$conversation_id"
                
                echo "\033[36m⏺\033[0m \033[90m[$(date '+%H:%M:%S')] Switched to conversation \033[1m${conversation_id}\033[0m"
            fi
        else
            echo "\033[31m✗\033[0m No conversations found"
        fi
        
        BUFFER=""
        CURSOR=${#BUFFER}
        zle reset-prompt
        return 0
    fi
    
    # Handle providers command specially
    if [[ "$user_action" == "provider" ]]; then
        _forge_select_and_set_config "show-providers" "provider" "Provider"
        BUFFER=""
        CURSOR=${#BUFFER}
        zle reset-prompt
        return 0
    fi
    
    # Handle models command specially
    if [[ "$user_action" == "model" ]]; then
        _forge_select_and_set_config "show-models" "model" "Model"
        BUFFER=""
        CURSOR=${#BUFFER}
        zle reset-prompt
        return 0
    fi
    
    # Check if input_text is empty - just set the active agent
    
    # Validate that the command exists in show-commands (if user_action is provided)
    if [[ -n "$user_action" ]]; then
        if [[ -n "$_FORGE_COMMANDS" ]]; then
            # Check if the user_action is in the list of valid commands
            if ! echo "$_FORGE_COMMANDS" | grep -q "^${user_action}\b"; then
                echo
                echo "\033[31m⏺\033[0m \033[90m[$(date '+%H:%M:%S')]\033[0m \033[1;31mERROR:\033[0m Command '\033[1m${user_action}\033[0m' not found"
                BUFFER=""
                CURSOR=${#BUFFER}
                zle reset-prompt
                return 0
            fi
        fi
    fi
    if [[ -z "$input_text" ]]; then
        echo
        FORGE_ACTIVE_AGENT="${user_action:-${FORGE_ACTIVE_AGENT}}"
        _forge_print_agent_message
        BUFFER=""
        CURSOR=0
        zle reset-prompt
        return 0
    fi
    
    # Generate conversation ID if needed (in parent shell context)
    if [[ -z "$FORGE_CONVERSATION_ID" ]]; then
        FORGE_CONVERSATION_ID=$($_FORGE_BIN --generate-conversation-id)
    fi
    
    # Set the active agent for this execution
    FORGE_ACTIVE_AGENT="${user_action:-${FORGE_ACTIVE_AGENT}}"
    
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