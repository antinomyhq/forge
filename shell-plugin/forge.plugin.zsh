#!/usr/bin/env zsh

# Documentation in [README.md](./README.md)


# Configuration: Change these variables to customize the forge command and special characters
# Using typeset to keep variables local to plugin scope and prevent public exposure
typeset -h _FORGE_BIN="${FORGE_BIN:-forge}"
typeset -h _FORGE_CONVERSATION_PATTERN=":"
typeset -h _FORGE_MAX_COMMIT_DIFF="${FORGE_MAX_COMMIT_DIFF:-100000}"
typeset -h _FORGE_DELIMITER='\s\s+'

# Detect fd command - Ubuntu/Debian use 'fdfind', others use 'fd'
typeset -h _FORGE_FD_CMD="$(command -v fdfind 2>/dev/null || command -v fd 2>/dev/null || echo 'fd')"

# Detect bat command - use bat if available, otherwise fall back to cat
if command -v bat &>/dev/null; then
    typeset -h _FORGE_CAT_CMD="bat --color=always --style=numbers,changes --line-range=:500"
else
    typeset -h _FORGE_CAT_CMD="cat"
fi

# Commands cache - loaded lazily on first use
typeset -h _FORGE_COMMANDS=""

# Store active agent ID in a local variable (session-scoped)
# Default to "forge" agent
typeset -h _FORGE_ACTIVE_AGENT="forge"

# Store conversation ID in a temporary variable (local to plugin)
typeset -h _FORGE_CONVERSATION_ID=""

# Style the conversation pattern with appropriate highlighting
# Keywords in yellow, rest in default white

# Style tagged files
ZSH_HIGHLIGHT_PATTERNS+=('@\[[^]]#\]' 'fg=cyan,bold')

# Highlight colon + command name (supports letters, numbers, hyphens, underscores) in yellow
ZSH_HIGHLIGHT_PATTERNS+=('(#s):[a-zA-Z0-9_-]#' 'fg=yellow,bold')

# Highlight everything after the command name + space in white
ZSH_HIGHLIGHT_PATTERNS+=('(#s):[a-zA-Z0-9_-]# [[:graph:]]*' 'fg=white')

ZSH_HIGHLIGHT_HIGHLIGHTERS+=(pattern)

# Lazy loader for commands cache
# Loads the commands list only when first needed, avoiding startup cost
function _forge_get_commands() {
    if [[ -z "$_FORGE_COMMANDS" ]]; then
        _FORGE_COMMANDS="$(CLICOLOR_FORCE=0 $_FORGE_BIN list commands --porcelain 2>/dev/null)"
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
    # Invoke precmd hooks to ensure prompt customizations (starship, oh-my-zsh themes, etc.) refresh properly
    for precmd in $precmd_functions; do
        if typeset -f "$precmd" >/dev/null 2>&1; then
            "$precmd"
        fi
    done

   BUFFER=""
   CURSOR=0

   zle reset-prompt 
    
}

# Helper function to print messages with consistent formatting based on log level
# Usage: _forge_log <level> <message>
# Levels: error, info, success, warning, debug
# Color scheme matches crates/forge_main/src/title_display.rs
function _forge_log() {
    local level="$1"
    local message="$2"
    local timestamp="\033[90m[$(date '+%H:%M:%S')]\033[0m"
    
    case "$level" in
        error)
            # Category::Error - Red ❌
            echo "\033[31m❌\033[0m ${timestamp} \033[31m${message}\033[0m"
            ;;
        info)
            # Category::Info - White ⏺
            echo "\033[37m⏺\033[0m ${timestamp} \033[37m${message}\033[0m"
            ;;
        success)
            # Category::Action/Completion - Yellow ⏺
            echo "\033[33m⏺\033[0m ${timestamp} \033[37m${message}\033[0m"
            ;;
        warning)
            # Category::Warning - Bright yellow ⚠️
            echo "\033[93m⚠️\033[0m ${timestamp} \033[93m${message}\033[0m"
            ;;
        debug)
            # Category::Debug - Cyan ⏺ with dimmed text
            echo "\033[36m⏺\033[0m ${timestamp} \033[90m${message}\033[0m"
            ;;
        *)
            echo "${message}"
            ;;
    esac
}

# Helper function to find the index of a value in a list (1-based)
# Returns the index if found, 1 otherwise
# Usage: _forge_find_index <output> <value_to_find> [field_number]
# field_number: which field to compare (1 for first field, 2 for second field, etc.)
function _forge_find_index() {
    local output="$1"
    local value_to_find="$2"
    local field_number="${3:-1}"  # Default to first field if not specified

    local index=1
    while IFS= read -r line; do
        # Extract the specified field for comparison
        local field_value=$(echo "$line" | awk "{print \$$field_number}")
        if [[ "$field_value" == "$value_to_find" ]]; then
            echo "$index"
            return 0
        fi
        ((index++))
    done <<< "$output"

    echo "1"
    return 0
}

# Helper function to select a provider from the list
# Usage: _forge_select_provider [filter_status] [current_provider]
# Returns: selected provider line (via stdout)
function _forge_select_provider() {
    local filter_status="${1:-}"
    local current_provider="${2:-}"
    local output
    output=$($_FORGE_BIN list provider --porcelain 2>/dev/null)
    
    if [[ -z "$output" ]]; then
        _forge_log error "No providers available"
        return 1
    fi
    
    # Filter by status if specified (e.g., "available" for configured providers)
    if [[ -n "$filter_status" ]]; then
        output=$(echo "$output" | grep -i "$filter_status")
        if [[ -z "$output" ]]; then
            _forge_log error "No ${filter_status} providers found"
            return 1
        fi
    fi
    
    # Get current provider if not provided
    if [[ -z "$current_provider" ]]; then
        current_provider=$($_FORGE_BIN config get provider --porcelain 2>/dev/null)
    fi
    
    local fzf_args=(
        --delimiter="$_FORGE_DELIMITER"
        --prompt="Provider ❯ "
        --with-nth=1,3..
    )
    
    # Position cursor on current provider if available
    if [[ -n "$current_provider" ]]; then
        # For providers, compare against the first field (display name)
        local index=$(_forge_find_index "$output" "$current_provider" 1)
        fzf_args+=(--bind="start:pos($index)")
    fi
    
    local selected
    selected=$(echo "$output" | _forge_fzf "${fzf_args[@]}")
    
    if [[ -n "$selected" ]]; then
        echo "$selected"
        return 0
    fi
    
    return 1
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
                # For models, compare against the first field (model_id)
                local index=$(_forge_find_index "$output" "$default_value" 1)
                
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
        _forge_log error "No active conversation. Start a conversation first or use :list to see existing ones"
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
    
    # Handle @ completion (files and directories)
    if [[ "$current_word" =~ ^@.*$ ]]; then
        local filter_text="${current_word#@}"
        local selected
        local fzf_args=(
            --preview="if [ -d {} ]; then ls -la --color=always {} 2>/dev/null || ls -la {}; else $_FORGE_CAT_CMD {}; fi"
            --preview-window=right:60%:wrap:border-sharp
        )
        
        if [[ -n "$filter_text" ]]; then
            selected=$($_FORGE_FD_CMD --type f --type d --hidden --exclude .git | _forge_fzf --query "$filter_text" "${fzf_args[@]}")
        else
            selected=$($_FORGE_FD_CMD --type f --type d --hidden --exclude .git | _forge_fzf "${fzf_args[@]}")
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
    
    # Handle :command completion (supports letters, numbers, hyphens, underscores)
    if [[ "${LBUFFER}" =~ "^:([a-zA-Z][a-zA-Z0-9_-]*)?$" ]]; then
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
        _forge_handle_conversation_command "dump" "--html"
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
    local input_text="$1"
    
    echo
    
    # If an ID is provided directly, use it
    if [[ -n "$input_text" ]]; then
        local conversation_id="$input_text"
        
        # Set the conversation as active
        _FORGE_CONVERSATION_ID="$conversation_id"
        
        # Show conversation content
        echo
        _forge_exec conversation show "$conversation_id"
        
        # Show conversation info
        _forge_exec conversation info "$conversation_id"
        
        # Print log about conversation switching
        _forge_log success "Switched to conversation \033[1m${conversation_id}\033[0m"
        
        _forge_reset
        return 0
    fi
    
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
            # For conversations, compare against the first field (conversation_id)
            local index=$(_forge_find_index "$conversations_output" "$current_id" 1)
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
            _forge_log success "Switched to conversation \033[1m${conversation_id}\033[0m"
            
        fi
    else
        _forge_log error "No conversations found"
    fi
    
    _forge_reset
}

# Action handler: Select agent
function _forge_action_agent() {
    local input_text="$1"
    
    echo
    
    # If an agent ID is provided directly, use it
    if [[ -n "$input_text" ]]; then
        local agent_id="$input_text"
        
        # Validate that the agent exists
        local agent_exists=$($_FORGE_BIN list agents --porcelain 2>/dev/null | grep -q "^${agent_id}\b" && echo "true" || echo "false")
        if [[ "$agent_exists" == "false" ]]; then
            _forge_log error "Agent '\033[1m${agent_id}\033[0m' not found"
            _forge_reset
            return 0
        fi
        
        # Set the agent as active
        _FORGE_ACTIVE_AGENT="$agent_id"
        
        # Print log about agent switching
        _forge_log success "Switched to agent \033[1m${agent_id}\033[0m"
        
        _forge_reset
        return 0
    fi
    
    # Get agents list
    local agents_output
    agents_output=$($_FORGE_BIN list agents --porcelain 2>/dev/null)
    
    if [[ -n "$agents_output" ]]; then
        # Get current agent ID
        local current_agent="$_FORGE_ACTIVE_AGENT"
        
        # Sort agents alphabetically by name (first field)
        local sorted_agents=$(echo "$agents_output" | sort)
        
        # Create prompt with current agent - show agent ID, title, provider, model and reasoning
        local prompt_text="Agent ❯ "
        local fzf_args=(
            --prompt="$prompt_text"
            --delimiter="$_FORGE_DELIMITER"
            --with-nth="1,2,4,5,6"
        )

        # If there's a current agent, position cursor on it
        if [[ -n "$current_agent" ]]; then
            local index=$(_forge_find_index "$sorted_agents" "$current_agent")
            fzf_args+=(--bind="start:pos($index)")
        fi

        local selected_agent
        # Use fzf without preview for simple selection like provider/model
        selected_agent=$(echo "$sorted_agents" | _forge_fzf "${fzf_args[@]}")
        
        if [[ -n "$selected_agent" ]]; then
            # Extract the first field (agent ID)
            local agent_id=$(echo "$selected_agent" | awk '{print $1}')
            
            # Set the selected agent as active
            _FORGE_ACTIVE_AGENT="$agent_id"
            
            # Print log about agent switching
            _forge_log success "Switched to agent \033[1m${agent_id}\033[0m"
            
        fi
    else
        _forge_log error "No agents found"
    fi
    
    _forge_reset
}

# Action handler: Select provider# Action handler: Select provider
function _forge_action_provider() {
    echo
    local selected
    selected=$(_forge_select_provider)
    
    if [[ -n "$selected" ]]; then
        # Extract the second field (provider ID) from the selected line
        # Format: "DisplayName  provider_id  host  status"
        local provider_id=$(echo "$selected" | awk '{print $2}')
        # Always use config set - it will handle authentication if needed
        _forge_exec config set provider "$provider_id"
    fi
    _forge_reset
}

# Action handler: Select model
function _forge_action_model() {
    _forge_select_and_set_config "list models" "model" "Model" "$($_FORGE_BIN config get model --porcelain)" "2,3.."
    _forge_reset
}

# Action handler: Commit changes with AI-generated message
# Usage: :commit [additional context]
function _forge_action_commit() {
    local additional_context="$1"
    local commit_message
    # Generate AI commit message
    echo
    # Force color output even when not connected to TTY
    # FORCE_COLOR: for indicatif spinner colors
    # CLICOLOR_FORCE: for colored crate text colors
    
    # Build commit command with optional additional context
    if [[ -n "$additional_context" ]]; then
        commit_message=$(FORCE_COLOR=true CLICOLOR_FORCE=1 $_FORGE_BIN commit --preview --max-diff "$_FORGE_MAX_COMMIT_DIFF" $additional_context)
    else
        commit_message=$(FORCE_COLOR=true CLICOLOR_FORCE=1 $_FORGE_BIN commit --preview --max-diff "$_FORGE_MAX_COMMIT_DIFF")
    fi
    
    # Proceed only if command succeeded
    if [[ -n "$commit_message" ]]; then
        # Check if there are staged changes to determine commit strategy
        if git diff --staged --quiet; then
            # No staged changes: commit all tracked changes with -a flag
            BUFFER="git commit -a -m '$commit_message'"
        else
            # Staged changes exist: commit only what's staged
            BUFFER="git commit -m '$commit_message'"
        fi
        # Move cursor to end of buffer for immediate execution
        CURSOR=${#BUFFER}
        # Refresh display to show the new command
        zle reset-prompt
    else
        echo "$commit_message"
        _forge_reset
    fi
}

# Action handler: Show tools
function _forge_action_tools() {
    echo
    # Ensure FORGE_ACTIVE_AGENT always has a value, default to "forge"
    local agent_id="${_FORGE_ACTIVE_AGENT:-forge}"
    _forge_exec list tools "$agent_id"
    _forge_reset
}


# Action handler: Open external editor for command composition
function _forge_action_editor() {
    local initial_text="$1"
    echo
    
    # Determine editor in order of preference: FORGE_EDITOR > EDITOR > nano
    local editor_cmd="${FORGE_EDITOR:-${EDITOR:-nano}}"
    
    # Validate editor exists
    if ! command -v "${editor_cmd%% *}" &>/dev/null; then
        _forge_log error "Editor not found: $editor_cmd (set FORGE_EDITOR or EDITOR)"
        _forge_reset
        return 1
    fi
    
    # Create .forge directory if it doesn't exist
    local forge_dir=".forge"
    if [[ ! -d "$forge_dir" ]]; then
        mkdir -p "$forge_dir" || {
            _forge_log error "Failed to create .forge directory"
            _forge_reset
            return 1
        }
    fi
    
    # Create temporary file with git-like naming: FORGE_EDITMSG
    local temp_file="${forge_dir}/FORGE_EDITMSG"
    touch "$temp_file" || {
        _forge_log error "Failed to create temporary file"
        _forge_reset
        return 1
    }
    
    # Ensure cleanup on exit
    trap "rm -f '$temp_file'" EXIT INT TERM
    
    # Pre-populate with initial text if provided
    if [[ -n "$initial_text" ]]; then
        echo "$initial_text" > "$temp_file"
    fi
    
    # Open editor
    eval "$editor_cmd '$temp_file'"
    local editor_exit_code=$?
    
    if [ $editor_exit_code -ne 0 ]; then
        _forge_log error "Editor exited with error code $editor_exit_code"
        _forge_reset
        return 1
    fi
    
    # Read and process content
    local content
    content=$(cat "$temp_file" | tr -d '\r')
    
    if [ -z "$content" ]; then
        _forge_log info "Editor closed with no content"
        _forge_reset
        return 0
    fi
    
    # Insert into buffer with : prefix
    BUFFER=": $content"
    CURSOR=${#BUFFER}
    
    _forge_log info "Command ready - press Enter to execute"
    zle reset-prompt
}

# Action handler: Show skills
function _forge_action_skill() {
    echo
    _forge_exec list skill
    _forge_reset
}

# Action handler: Generate shell command from natural language
# Usage: :? <description>
function _forge_action_suggest() {
    local description="$1"
    
    if [[ -z "$description" ]]; then
        _forge_log error "Please provide a command description"
        _forge_reset
        return 0
    fi
    
    echo
    # Generate the command
    local generated_command
    generated_command=$(FORCE_COLOR=true CLICOLOR_FORCE=1 _forge_exec suggest "$description")
    
    if [[ -n "$generated_command" ]]; then
        # Replace the buffer with the generated command
        BUFFER="$generated_command"
        CURSOR=${#BUFFER}
        zle reset-prompt
    else
        _forge_log error "Failed to generate command"
        _forge_reset
    fi
}

# Action handler: Login to provider
function _forge_action_login() {
    echo
    local selected
    selected=$(_forge_select_provider)
    if [[ -n "$selected" ]]; then
        # Extract the second field (provider ID)
        local provider=$(echo "$selected" | awk '{print $2}')
        _forge_exec provider login "$provider"
    fi
    _forge_reset
}

# Action handler: Logout from provider
function _forge_action_logout() {
    echo
    local selected
    selected=$(_forge_select_provider "available")
    if [[ -n "$selected" ]]; then
        # Extract the second field (provider ID)
        local provider=$(echo "$selected" | awk '{print $2}')
        _forge_exec provider logout "$provider"
    fi
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
            # Check if the user_action is in the list of valid commands and extract the row
            local command_row=$(echo "$commands_list" | grep "^${user_action}\b")
            if [[ -z "$command_row" ]]; then
                echo
                _forge_log error "Command '\033[1m${user_action}\033[0m' not found"
                _forge_reset
                return 0
            fi
            
            # Extract the command type from the last field of the row
            local command_type="${command_row##* }"
            if [[ "$command_type" == "custom" ]]; then
                # Generate conversation ID if needed
                [[ -z "$_FORGE_CONVERSATION_ID" ]] && _FORGE_CONVERSATION_ID=$($_FORGE_BIN conversation new)
                
                echo
                # Execute custom command with run subcommand
                if [[ -n "$input_text" ]]; then
                    _forge_exec cmd --cid "$_FORGE_CONVERSATION_ID" "$user_action" "$input_text"
                else
                    _forge_exec cmd --cid "$_FORGE_CONVERSATION_ID" "$user_action"
                fi
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
            _forge_log info "\033[1;37m${_FORGE_ACTIVE_AGENT:u}\033[0m \033[90mis now the active agent\033[0m"
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
    
    # ⚠️  IMPORTANT: When adding a new command here, you MUST also update:
    #     crates/forge_main/src/built_in_commands.json
    #     Add a new entry: {"command": "name", "description": "Description [alias: x]"}
    #
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
        dump|d)
            _forge_action_dump "$input_text"
        ;;
        compact)
            _forge_action_compact
        ;;
        retry|r)
            _forge_action_retry
        ;;
        agent|a)
            _forge_action_agent "$input_text"
        ;;
        conversation|c)
            _forge_action_conversation "$input_text"
        ;;
        provider|p)
            _forge_action_provider
        ;;
        model|m)
            _forge_action_model
        ;;
        tools|t)
            _forge_action_tools
        ;;
        skill)
            _forge_action_skill
        ;;
        edit|ed)
            _forge_action_editor "$input_text"
        ;;
        commit)
            _forge_action_commit "$input_text"
        ;;
        suggest|s)
            _forge_action_suggest "$input_text"
        ;;
        login)
            _forge_action_login
        ;;
        logout)
            _forge_action_logout
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

#################################################################################
# POWERLEVEL10K INTEGRATION
#################################################################################
# Automatically configure Powerlevel10k prompt segments for Forge
# This section only runs if Powerlevel10k is detected (p10k command exists)

if (( $+functions[p10k] )) || [[ -n "$POWERLEVEL9K_MODE" ]]; then

  #################################[ forge_agent: forge active agent ]#################################
  # Custom segment to display the currently active Forge agent
  # This function runs on every prompt render to show which agent is handling tasks
  #
  # POSITIONING:
  # - Added to POWERLEVEL9K_LEFT_PROMPT_ELEMENTS as the FIRST item
  # - Appears on the far LEFT of your prompt in BOLD UPPERCASE
  #
  # COLOR:
  # - DIMMED GRAY (242) when no active conversation (_FORGE_CONVERSATION_ID is empty)
  # - WHITE (231) when there's an active conversation
  function prompt_forge_agent() {
    # Check if $_FORGE_ACTIVE_AGENT environment variable is set
    if [[ -n "$_FORGE_ACTIVE_AGENT" ]]; then
      # Convert the agent name to UPPERCASE using ${(U)variable} syntax
      local agent_upper="${(U)_FORGE_ACTIVE_AGENT}"
      
      # Determine color based on conversation state:
      # - 242 (dimmed gray) = no active conversation
      # - 231 (white) = active conversation
      local segment_color=242
      if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
        segment_color=231
      fi
      
      # Display the prompt segment using p10k:
      # -f $segment_color : Set foreground color based on conversation state
      # -t "$agent_upper" : Set the text content to the uppercase agent name
      p10k segment -f $segment_color -t "$agent_upper"
    fi
  }

  # Instant prompt version of forge_agent
  # This enables the segment to appear in instant prompt (fast startup mode)
  function instant_prompt_forge_agent() {
    prompt_forge_agent
  }

  # Customization: Make the forge_agent text BOLD
  # %B = Start bold, %b = End bold
  # Color is handled dynamically by prompt_forge_agent function
  typeset -g POWERLEVEL9K_FORGE_AGENT_CONTENT_EXPANSION='%B${(U)_FORGE_ACTIVE_AGENT}%b'

  #################################[ forge_model: forge current model ]#################################
  # Custom segment to display the current forge model configuration
  # This function runs on every prompt render to show which AI model is currently active
  #
  # POSITIONING:
  # - Added to POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS as the FIRST item
  # - Appears on the far RIGHT of your prompt
  #
  # COLOR:
  # - DIMMED GRAY (242) when no active conversation (_FORGE_CONVERSATION_ID is empty)
  # - CYAN (39) when there's an active conversation
  #
  # INDICATOR:
  # - ○ (empty circle) when idle (no conversation)
  # - ● (filled circle) when active (conversation in progress)
  function prompt_forge_model() {
    local model_output
    
    # Determine which forge binary to use:
    # 1. First try _FORGE_BIN (plugin internal variable)
    # 2. Then try FORGE_BIN environment variable (for development/debugging)
    # 3. Fall back to 'forge' command in PATH (for production)
    local forge_cmd="${_FORGE_BIN:-${FORGE_BIN:-forge}}"
    
    # Execute 'forge config get model' to retrieve the current model
    # Suppress errors (2>/dev/null) to avoid cluttering the prompt if forge isn't available
    model_output=$($forge_cmd config get model 2>/dev/null)
    
    # Only display the segment if we successfully got a model name
    if [[ -n "$model_output" ]]; then
      # Determine color and indicator based on conversation state:
      # - 242 (dimmed gray) + ○ = no active conversation (idle)
      # - 39 (cyan) + ● = active conversation
      local segment_color=242
      local indicator="○"
      if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
        segment_color=39
        indicator="●"
      fi
      
      # Display the prompt segment using p10k:
      # -f $segment_color : Set foreground color based on conversation state
      # -i '$indicator'   : Display conversation indicator (○ idle or ● active)
      # -t "$model_output" : Set the text content to the model name
      p10k segment -f $segment_color -i "$indicator" -t "$model_output"
    fi
  }

  # Instant prompt version of forge_model
  # This enables the segment to appear in instant prompt (fast startup mode)
  function instant_prompt_forge_model() {
    prompt_forge_model
  }

  #################################[ Update Prompt Elements ]#################################
  # Prepend forge_agent to LEFT prompt (appears first/leftmost)
  # Only add if not already present
  if [[ ! " ${POWERLEVEL9K_LEFT_PROMPT_ELEMENTS[*]} " =~ " forge_agent " ]]; then
    POWERLEVEL9K_LEFT_PROMPT_ELEMENTS=(forge_agent "${POWERLEVEL9K_LEFT_PROMPT_ELEMENTS[@]}")
  fi

  # Prepend forge_model to RIGHT prompt (appears first/rightmost)
  # Only add if not already present
  if [[ ! " ${POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS[*]} " =~ " forge_model " ]]; then
    POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS=(forge_model "${POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS[@]}")
  fi

fi
# End of Powerlevel10k integration

#################################################################################
# PLAIN ZSH PROMPT INTEGRATION
#################################################################################
# Automatically configure zsh prompt for Forge (for users without Powerlevel10k)
# This section only runs if Powerlevel10k is NOT detected

if ! (( $+functions[p10k] )) && [[ -z "$POWERLEVEL9K_MODE" ]]; then

  # Store original prompts to preserve user's existing configuration
  # We'll prepend/append our forge info to these
  typeset -g _FORGE_ORIGINAL_PROMPT="${PROMPT}"
  typeset -g _FORGE_ORIGINAL_RPROMPT="${RPROMPT}"

  #################################[ _forge_zsh_prompt_agent ]#################################
  # Returns the active agent formatted for display in PROMPT
  # Format: BOLD UPPERCASE agent name
  #
  # COLOR:
  # - DIMMED GRAY (242) when no active conversation (_FORGE_CONVERSATION_ID is empty)
  # - WHITE (231) when there's an active conversation
  function _forge_zsh_prompt_agent() {
    if [[ -n "$_FORGE_ACTIVE_AGENT" ]]; then
      # Determine color based on conversation state:
      # - 242 (dimmed gray) = no active conversation
      # - 231 (white) = active conversation
      local agent_color=242
      if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
        agent_color=231
      fi
      
      # %B = bold, %F{color} = set color, %f = reset foreground, %b = reset bold
      # ${(U)var} = uppercase the variable
      echo "%B%F{$agent_color}${(U)_FORGE_ACTIVE_AGENT}%f%b "
    fi
  }

  #################################[ _forge_zsh_prompt_model ]#################################
  # Returns the current model formatted for display in RPROMPT
  # Format: Indicator + model name
  #
  # COLOR:
  # - DIMMED GRAY (242) when no active conversation (_FORGE_CONVERSATION_ID is empty)
  # - CYAN (39) when there's an active conversation
  #
  # INDICATOR:
  # - ○ (empty circle) when idle (no conversation)
  # - ● (filled circle) when active (conversation in progress)
  function _forge_zsh_prompt_model() {
    local forge_cmd="${_FORGE_BIN:-${FORGE_BIN:-forge}}"
    local model_output
    model_output=$($forge_cmd config get model 2>/dev/null)
    
    if [[ -n "$model_output" ]]; then
      # Determine color and indicator based on conversation state:
      # - 242 (dimmed gray) + ○ = no active conversation (idle)
      # - 39 (cyan) + ● = active conversation
      local segment_color=242
      local indicator="○"
      if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
        segment_color=39
        indicator="●"
      fi
      
      # %F{color} = set foreground color, %f = reset foreground
      echo "%F{$segment_color}${indicator} ${model_output}%f"
    fi
  }

  #################################[ _forge_zsh_precmd ]#################################
  # Precmd hook that updates PROMPT and RPROMPT before each prompt display
  # This ensures the agent and model are always current
  function _forge_zsh_precmd() {
    # Build LEFT prompt: [AGENT] + original prompt
    # Only add agent prefix if _FORGE_ACTIVE_AGENT is set
    if [[ -n "$_FORGE_ACTIVE_AGENT" ]]; then
      PROMPT="$(_forge_zsh_prompt_agent)${_FORGE_ORIGINAL_PROMPT}"
    else
      PROMPT="${_FORGE_ORIGINAL_PROMPT}"
    fi
    
    # Build RIGHT prompt: original rprompt + [MODEL]
    local model_segment="$(_forge_zsh_prompt_model)"
    if [[ -n "$model_segment" ]]; then
      if [[ -n "$_FORGE_ORIGINAL_RPROMPT" ]]; then
        RPROMPT="${_FORGE_ORIGINAL_RPROMPT} ${model_segment}"
      else
        RPROMPT="${model_segment}"
      fi
    else
      RPROMPT="${_FORGE_ORIGINAL_RPROMPT}"
    fi
  }

  # Register the precmd hook
  # Using add-zsh-hook if available (from zsh/hooks), otherwise append to precmd_functions
  if (( $+functions[add-zsh-hook] )); then
    add-zsh-hook precmd _forge_zsh_precmd
  else
    precmd_functions+=(_forge_zsh_precmd)
  fi

fi
# End of Plain ZSH integration
