#!/usr/bin/env zsh

# Documentation in [README.md](./README.md)


# Configuration: Change these variables to customize the forge command and special characters
# Using typeset to keep variables local to plugin scope and prevent public exposure
typeset -h _FORGE_BIN="${FORGE_BIN:-forge}"
typeset -h _FORGE_CONVERSATION_PATTERN=":"
typeset -h _FORGE_DELIMITER='\s\s+'

# Detect fd command - Ubuntu/Debian use 'fdfind', others use 'fd'
typeset -h _FORGE_FD_CMD="$(command -v fdfind 2>/dev/null || command -v fd 2>/dev/null || echo 'fd')"

# Commands cache - loaded lazily on first use
typeset -h _FORGE_COMMANDS=""

# Store active agent ID in a local variable (session-scoped)
# Default to "forge" agent
typeset -h _FORGE_ACTIVE_AGENT="forge"

# Store conversation ID in a temporary variable (local to plugin)
typeset -h _FORGE_CONVERSATION_ID=""

# Style tagged files
ZSH_HIGHLIGHT_PATTERNS+=('@\[[^]]#\]' 'fg=cyan,bold')

ZSH_HIGHLIGHT_HIGHLIGHTERS+=(pattern)
# Style the conversation pattern with appropriate highlighting
# Keywords in yellow, rest in default white

# Highlight colon + word at the beginning in yellow
ZSH_HIGHLIGHT_PATTERNS+=('(#s):[a-zA-Z]#' 'fg=yellow,bold')

# Highlight everything after that word + space in white
ZSH_HIGHLIGHT_PATTERNS+=('(#s):[a-zA-Z]# *(*|[[:graph:]]*)' 'fg=white,bold')

# Lazy loader for commands cache
# Loads the commands list only when first needed, avoiding startup cost
function _forge_get_commands() {
    if [[ -z "$_FORGE_COMMANDS" ]]; then
        _FORGE_COMMANDS="$($_FORGE_BIN list commands --porcelain 2>/dev/null)"
    fi
    echo "$_FORGE_COMMANDS"
}

# Private fzf function with common options for consistent UX
function _forge_fzf() {
    fzf --exact --cycle --select-1 --height 100% --reverse --no-scrollbar "$@"
}

# Helper function to execute forge commands consistently
# This ensures proper handling of special characters and consistent output
function _forge_exec() {
    # Ensure FORGE_ACTIVE_AGENT always has a value, default to "forge"
    local agent_id="${_FORGE_ACTIVE_AGENT:-forge}"
    
    eval "$_FORGE_BIN --agent $(printf '%q' "$agent_id") $(printf '%q ' "$@")"
}

# Helper function to clear buffer and reset prompt
function _forge_reset() {
    BUFFER=""
    CURSOR=${#BUFFER}
    zle reset-prompt
}

# Helper function to print operating agent messages with consistent formatting
function _forge_print_agent_message() {
    # Ensure FORGE_ACTIVE_AGENT always has a value, default to "forge"
    local agent_name="${_FORGE_ACTIVE_AGENT:-forge}"
    echo "\033[33m⏺\033[0m \033[90m[$(date '+%H:%M:%S')] \033[1;37m${agent_name:u}\033[0m \033[90mis the active agent\033[0m"
}

# Helper function to find the index of a value in a list (1-based)
# Returns the index if found, 1 otherwise
function _forge_find_index() {
    local output="$1"
    local value_to_find="$2"

    local index=1
    while IFS= read -r line; do
        local name="${line%% *}"
        if [[ "$name" == "$value_to_find" ]]; then
            echo "$index"
            return 0
        fi
        ((index++))
    done <<< "$output"

    echo "1"
    return 0
}

# Helper function to select and set config values with fzf
function _forge_select_and_set_config() {
    local show_command="$1"
    local config_flag="$2"
    local prompt_text="$3"
    local default_value="$4"
    local with_nth="${5:-}"  # Optional column selection parameter
    (
        echo
        local output
        # Handle multi-word commands properly
        if [[ "$show_command" == *" "* ]]; then
            # Split the command into words and execute with --porcelain
            local cmd_parts=(${=show_command})
            output=$($_FORGE_BIN "${cmd_parts[@]}" --porcelain 2>/dev/null)
        else
            output=$($_FORGE_BIN "$show_command" --porcelain 2>/dev/null)
        fi
        
        if [[ -n "$output" ]]; then
            local selected
            local fzf_args=(--delimiter="$_FORGE_DELIMITER" --prompt="$prompt_text ❯ ")

            if [[ -n "$with_nth" ]]; then
                fzf_args+=(--with-nth="$with_nth")
            fi

            if [[ -n "$default_value" ]]; then
                local index=$(_forge_find_index "$output" "$default_value")
                
                fzf_args+=(--bind="start:pos($index)")
                
            fi
            selected=$(echo "$output" | _forge_fzf "${fzf_args[@]}")

            if [[ -n "$selected" ]]; then
                local name="${selected%% *}"
                _forge_exec config set "$config_flag" "$name"
            fi
        fi
    )
}


# Helper function to handle conversation commands that require an active conversation
function _forge_handle_conversation_command() {
    local subcommand="$1"
    shift  # Remove first argument, remaining args become extra parameters
    
    echo
    
    # Check if FORGE_CONVERSATION_ID is set
    if [[ -z "$_FORGE_CONVERSATION_ID" ]]; then
        echo "\033[31m✗\033[0m No active conversation. Start a conversation first or use :list to see existing ones"
        _forge_reset
        return 0
    fi
    
    # Execute the conversation command with conversation ID and any extra arguments
    _forge_exec conversation "$subcommand" "$_FORGE_CONVERSATION_ID" "$@"
    
    _forge_reset
    return 0
}

# Custom completion widget that handles both :commands and @ completion
function forge-completion() {
    local current_word="${LBUFFER##* }"
    
    # Handle @ completion (existing functionality)
    if [[ "$current_word" =~ ^@.*$ ]]; then
        local filter_text="${current_word#@}"
        local selected
        local fzf_args=(
            --preview="bat --color=always --style=numbers,changes --line-range=:500 {} 2>/dev/null || cat {}"
            --preview-window=right:60%:wrap:border-sharp
        )
        
        if [[ -n "$filter_text" ]]; then
            selected=$($_FORGE_FD_CMD --type f --hidden --exclude .git | _forge_fzf --query "$filter_text" "${fzf_args[@]}")
        else
            selected=$($_FORGE_FD_CMD --type f --hidden --exclude .git | _forge_fzf "${fzf_args[@]}")
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
        
        # Lazily load the commands list
        local commands_list=$(_forge_get_commands)
        if [[ -n "$commands_list" ]]; then
            # Use fzf for interactive selection with prefilled filter
            local selected
            if [[ -n "$filter_text" ]]; then
                selected=$(echo "$commands_list" | _forge_fzf --delimiter="$_FORGE_DELIMITER" --nth=1 --query "$filter_text" --prompt="Command ❯ ")
            else
                selected=$(echo "$commands_list" | _forge_fzf --delimiter="$_FORGE_DELIMITER" --nth=1 --prompt="Command ❯ ")
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

# Action handler: Start a new conversation
function _forge_action_new() {
    _FORGE_CONVERSATION_ID=""
    _FORGE_ACTIVE_AGENT="forge"
    
    echo
    _forge_exec banner
    _forge_print_agent_message
    _forge_reset
}

# Action handler: Show session info
function _forge_action_info() {
    echo
    if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
        _forge_exec info --cid "$_FORGE_CONVERSATION_ID"
    else
        _forge_exec info
    fi
    _forge_reset
}

# Action handler: Show environment info
function _forge_action_env() {
    echo
    _forge_exec env
    _forge_reset
}

# Action handler: Dump conversation
function _forge_action_dump() {
    local input_text="$1"
    if [[ "$input_text" == "html" ]]; then
        _forge_handle_conversation_command "dump" "html"
    else
        _forge_handle_conversation_command "dump"
    fi
}

# Action handler: Compact conversation
function _forge_action_compact() {
    _forge_handle_conversation_command "compact"
}

# Action handler: Retry last message
function _forge_action_retry() {
    _forge_handle_conversation_command "retry"
}

# Action handler: List/switch conversations
function _forge_action_conversation() {
    echo
    
    # Get conversations list
    local conversations_output
    conversations_output=$($_FORGE_BIN conversation list --porcelain 2>/dev/null)
    
    if [[ -n "$conversations_output" ]]; then
        # Get current conversation ID if set
        local current_id="$_FORGE_CONVERSATION_ID"
        
        # Create prompt with current conversation
        local prompt_text="Conversation ❯ "
        local fzf_args=(
            --prompt="$prompt_text"
            --delimiter="$_FORGE_DELIMITER"
            --with-nth="2,3"
            --preview="CLICOLOR_FORCE=1 $_FORGE_BIN conversation info {1}; echo; CLICOLOR_FORCE=1 $_FORGE_BIN conversation show {1}"
            --preview-window=right:60%:wrap:border-sharp
        )

        # If there's a current conversation, position cursor on it
        if [[ -n "$current_id" ]]; then
            local index=$(_forge_find_index "$conversations_output" "$current_id")
            fzf_args+=(--bind="start:pos($index)")
        fi

        local selected_conversation
        # Use fzf with preview showing the last message from the conversation
        selected_conversation=$(echo "$conversations_output" | _forge_fzf "${fzf_args[@]}")
        
        if [[ -n "$selected_conversation" ]]; then
            # Extract the first field (UUID) - everything before the first multi-space delimiter
            local conversation_id=$(echo "$selected_conversation" | sed -E 's/  .*//' | tr -d '\n')
            
            # Set the selected conversation as active (in parent shell)
            _FORGE_CONVERSATION_ID="$conversation_id"
            # Show conversation content
            echo
            _forge_exec conversation show "$conversation_id"
            
            # Show conversation info
            _forge_exec conversation info "$conversation_id"
            
            # Print log about conversation switching
            echo "\033[36m⏺\033[0m \033[90m[$(date '+%H:%M:%S')] Switched to conversation \033[1m${conversation_id}\033[0m"
            
        fi
    else
        echo "\033[31m✗\033[0m No conversations found"
    fi
    
    _forge_reset
}

# Action handler: Select provider
function _forge_action_provider() {
    _forge_select_and_set_config "list providers" "provider" "Provider" "$($_FORGE_BIN config get provider --porcelain)"
    _forge_reset
}

# Action handler: Select model
function _forge_action_model() {
    _forge_select_and_set_config "list models" "model" "Model" "$($_FORGE_BIN config get model --porcelain)" "2,3.."
    _forge_reset
}

# Action handler: Show tools
function _forge_action_tools() {
    echo
    # Ensure FORGE_ACTIVE_AGENT always has a value, default to "forge"
    local agent_id="${_FORGE_ACTIVE_AGENT:-forge}"
    _forge_exec list tools "$agent_id"
    _forge_reset
}

# Action handler: Set active agent or execute command
function _forge_action_default() {
    local user_action="$1"
    local input_text="$2"
    
    # Validate that the command exists in show-commands (if user_action is provided)
    if [[ -n "$user_action" ]]; then
        local commands_list=$(_forge_get_commands)
        if [[ -n "$commands_list" ]]; then
            # Check if the user_action is in the list of valid commands
            if ! echo "$commands_list" | grep -q "^${user_action}\b"; then
                echo
                echo "\033[31m⏺\033[0m \033[90m[$(date '+%H:%M:%S')]\033[0m \033[1;31mERROR:\033[0m Command '\033[1m${user_action}\033[0m' not found"
                _forge_reset
                return 0
            fi
        fi
    fi
    
    # If input_text is empty, just set the active agent (only if user explicitly specified one)
    if [[ -z "$input_text" ]]; then
        if [[ -n "$user_action" ]]; then
            echo
            # Set the agent in the local variable
            _FORGE_ACTIVE_AGENT="$user_action"
            echo "\033[33m⏺\033[0m \033[90m[$(date '+%H:%M:%S')] \033[1;37m${_FORGE_ACTIVE_AGENT:u}\033[0m \033[90mis now the active agent\033[0m"
        fi
        _forge_reset
        return 0
    fi
    
    # Generate conversation ID if needed (in parent shell context)
    if [[ -z "$_FORGE_CONVERSATION_ID" ]]; then
        _FORGE_CONVERSATION_ID=$($_FORGE_BIN conversation new)
    fi
    
    echo
    
    # Only set the agent if user explicitly specified one
    if [[ -n "$user_action" ]]; then
        _FORGE_ACTIVE_AGENT="$user_action"
    fi
    
    # Execute the forge command directly with proper escaping
    _forge_exec -p "$input_text" --cid "$_FORGE_CONVERSATION_ID"
    
    # Reset the prompt
    _forge_reset
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
    case "$user_action" in
        ask)
            user_action="sage"
        ;;
        plan)
            user_action="muse"
        ;;
    esac
    
    # Dispatch to appropriate action handler using pattern matching
    case "$user_action" in
        new|n)
            _forge_action_new
        ;;
        info|i)
            _forge_action_info
        ;;
        env|e)
            _forge_action_env
        ;;
        dump)
            _forge_action_dump "$input_text"
        ;;
        compact)
            _forge_action_compact
        ;;
        retry)
            _forge_action_retry
        ;;
        conversation)
            _forge_action_conversation
        ;;
        provider)
            _forge_action_provider
        ;;
        model)
            _forge_action_model
        ;;
        tools)
            _forge_action_tools
        ;;
        *)
            _forge_action_default "$user_action" "$input_text"
        ;;
    esac
}

# Register ZLE widgets
zle -N forge-accept-line
# Register completions
zle -N forge-completion

# Custom bracketed-paste handler to fix syntax highlighting after paste
function forge-bracketed-paste() {
    zle .$WIDGET "$@"
    zle reset-prompt
}

# Register the bracketed paste widget to fix highlighting on paste
zle -N bracketed-paste forge-bracketed-paste



# Bind Enter to our custom accept-line that transforms :commands
bindkey '^M' forge-accept-line
bindkey '^J' forge-accept-line
# Update the Tab binding to use the new completion widget
bindkey '^I' forge-completion  # Tab for both @ and :command completion
