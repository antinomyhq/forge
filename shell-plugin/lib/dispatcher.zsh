#!/usr/bin/env zsh

# Main command dispatcher and widget registration

# ZLE widget for rename command with minibuffer support
function _forge_rename_widget() {
    local input_text="$1"
    
    # If we have input_text, call the action directly
    if [[ -n "$input_text" ]]; then
        _forge_action_rename "$input_text"
        return
    fi
    
    # Get conversations list for fzf selection
    local conversations_output
    conversations_output=$($_FORGE_BIN conversation list --porcelain 2>/dev/null)
    
    if [[ -z "$conversations_output" ]]; then
        _forge_log error "No conversations found"
        return
    fi
    
    # Get current conversation ID if set
    local current_id="$_FORGE_CONVERSATION_ID"
    
    # Create fzf interface similar to :conversation
    local prompt_text="Rename Conversation ❯ "
    local fzf_args=(
        --prompt="$prompt_text"
        --delimiter="$_FORGE_DELIMITER"
        --with-nth="2,3"
        --preview="CLICOLOR_FORCE=1 $_FORGE_BIN conversation info {1}; echo; CLICOLOR_FORCE=1 $_FORGE_BIN conversation show {1}"
        $_FORGE_PREVIEW_WINDOW
    )

    # Position cursor on current conversation if available
    if [[ -n "$current_id" ]]; then
        local index=$(_forge_find_index "$conversations_output" "$current_id" 1)
        fzf_args+=(--bind="start:pos($index)")
    fi

    local selected_conversation
    selected_conversation=$(echo "$conversations_output" | _forge_fzf --header-lines=1 "${fzf_args[@]}")
    
    if [[ -n "$selected_conversation" ]]; then
        # Extract conversation ID
        local conversation_id=$(echo "$selected_conversation" | sed -E 's/  .*//' | tr -d '\n')
        
        # Now use minibuffer to get new title
        local new_title
        # Always use fallback to regular read since ZLE is not reliable
        printf "\033[37m⏺\033[0m \033[90m[%s]\033[0m \033[37mRename conversation to:\033[0m " "$(date '+%H:%M:%S')" >&2
        if [[ -t 0 ]]; then
            read -e -r new_title
        elif [[ -t /dev/tty ]]; then
            read -e -r new_title < /dev/tty
        else
            read -e -r new_title
        fi
        
        if [[ -n "$new_title" ]]; then
            # Set WIDGET to ensure minibuffer context in rename function
            local old_widget="$WIDGET"
            WIDGET="_forge_rename_widget"
            
            # Call the rename function with FORGE_INPUT
            local old_forge_input="$FORGE_INPUT"
            FORGE_INPUT="$new_title"
            _forge_rename_conversation_with_prompt "$conversation_id"
            
            # Restore original values
            WIDGET="$old_widget"
            FORGE_INPUT="$old_forge_input"
        else
            _forge_log info "Rename cancelled"
        fi
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
            # Update terminal title after agent change
            _forge_update_terminal_title
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
    
    # Update terminal title after command execution
    _forge_update_terminal_title
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
        # Only use match[3] if the second group (space + params) was actually matched
        if [[ -n "${match[2]}" ]]; then
            input_text="${match[3]}"
        else
            input_text=""
        fi
    elif [[ "$BUFFER" =~ "^: (.*)$" ]]; then
        # Default action with parameters: : something
        user_action=""
        input_text="${match[1]}"
    else
        # For non-:commands, use normal accept-line - only if ZLE is available
        if {
            [[ $- == *i* ]] &&
            autoload -Uz zle 2>/dev/null &&
            zle 2>/dev/null
        }; then
            zle accept-line 2>/dev/null || true
        fi
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
        clone)
            _forge_action_clone "$input_text"
        ;;
        rename|rn)
            # Use ZLE widget for rename to enable minibuffer input
            _forge_rename_widget
            ;;
        sync)
            _forge_action_sync
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
