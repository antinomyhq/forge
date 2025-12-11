#!/usr/bin/env zsh

# Conversation management action handlers

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
        
        # Update terminal title after switching conversation
        _forge_update_terminal_title
        
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
            $_FORGE_PREVIEW_WINDOW
        )

        # If there's a current conversation, position cursor on it
        if [[ -n "$current_id" ]]; then
            # For conversations, compare against the first field (conversation_id)
            local index=$(_forge_find_index "$conversations_output" "$current_id" 1)
            fzf_args+=(--bind="start:pos($index)")
        fi

        local selected_conversation
        # Use fzf with preview showing the last message from the conversation
        selected_conversation=$(echo "$conversations_output" | _forge_fzf --header-lines=1 "${fzf_args[@]}")
        
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
            
            # Update terminal title after switching conversation
            _forge_update_terminal_title
            
        fi
    else
        _forge_log error "No conversations found"
    fi
    
    _forge_reset
}

# Action handler: Clone conversation
function _forge_action_clone() {
    local input_text="$1"
    local clone_target="$input_text"
    
    echo
    
    # Handle explicit clone target if provided
    if [[ -n "$clone_target" ]]; then
        _forge_clone_and_switch "$clone_target"
        _forge_reset
        return 0
    fi
    
    # Get conversations list for fzf selection
    local conversations_output
    conversations_output=$($_FORGE_BIN conversation list --porcelain 2>/dev/null)
    
    if [[ -z "$conversations_output" ]]; then
        _forge_log error "No conversations found"
        _forge_reset
        return 0
    fi
    
    # Get current conversation ID if set
    local current_id="$_FORGE_CONVERSATION_ID"
    
    # Create fzf interface similar to :conversation
    local prompt_text="Clone Conversation ❯ "
    local fzf_args=(
        --prompt="$prompt_text"
        --delimiter="$_FORGE_DELIMITER"
        --with-nth="2,3"
        --preview="CLICOLOR_FORCE=1 $_FORGE_BIN conversation info {1}; echo; CLICOLOR_FORCE=1 $_FORGE_BIN conversation show {1}"
        $_FORGE_PREVIEW_WINDOW
    )

    # Position cursor on current conversation if available
    if [[ -n "$current_id" ]]; then
        local index=$(_forge_find_index "$conversations_output" "$current_id")
        fzf_args+=(--bind="start:pos($index)")
    fi

    local selected_conversation
    selected_conversation=$(echo "$conversations_output" | _forge_fzf --header-lines=1 "${fzf_args[@]}")
    
    if [[ -n "$selected_conversation" ]]; then
        # Extract conversation ID
        local conversation_id=$(echo "$selected_conversation" | sed -E 's/  .*//' | tr -d '\n')
        _forge_clone_and_switch "$conversation_id"
    fi
    
    _forge_reset
}

# Action handler: Rename conversation
function _forge_action_rename() {
    local input_text="$1"
    
    echo
    
    # Handle explicit rename target if provided
    if [[ -n "$input_text" ]]; then
        local conversation_id="$input_text"
        _forge_rename_conversation_with_prompt "$conversation_id"
        _forge_reset
        return 0
    fi
    
    # Get conversations list for fzf selection
    local conversations_output
    conversations_output=$($_FORGE_BIN conversation list --porcelain 2>/dev/null)
    
    if [[ -z "$conversations_output" ]]; then
        _forge_log error "No conversations found"
        _forge_reset
        return 0
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
        _forge_rename_conversation_with_prompt "$conversation_id"
    fi
    
    _forge_reset
}

# Helper function to rename a conversation
function _forge_rename_conversation_with_prompt() {
    local conversation_id="$1"
    
    # Get current conversation title for default
    local conversation_info
    conversation_info=$($_FORGE_BIN conversation info "$conversation_id" </dev/null 2>/dev/null)
    # Remove ANSI escape codes first, then extract title
    local clean_info=$(echo "$conversation_info" | sed 's/\x1b\[[0-9;]*m//g')
    # Extract title from output - handle both "title" format and "Title:" format
    local current_title=$(echo "$clean_info" | grep '^  title ' | sed 's/^  title //')
    
    # If no title found with exact match, try to get the line after "title" and extract the rest
    if [[ -z "$current_title" ]]; then
        current_title=$(echo "$clean_info" | awk '/^  title / {for(i=3;i<=NF;i++) printf "%s ", $i; print ""}' | sed 's/ *$//')
    fi
    
    # If still no title found, try the old "Title:" format
    if [[ -z "$current_title" ]]; then
        current_title=$(echo "$clean_info" | grep "Title:" | sed 's/Title: //' || echo "<untitled>")
    fi
    
    # Final fallback if still empty
    if [[ -z "$current_title" ]]; then
        current_title="<untitled>"
    fi
    
    # Check if FORGE_INPUT environment variable is set (highest priority)
    if [[ -n "$FORGE_INPUT" ]]; then
        local new_title="$FORGE_INPUT"
        _forge_log info "Using FORGE_INPUT: \"$new_title\""
    else
        # Always use fallback to regular read since ZLE is not reliable
        local new_title
        # Display prompt in forge style (matches rust dialoguer formatting)
        # Info category: white icon ⏺, white text, dimmed timestamp
        printf "\033[37m⏺\033[0m \033[90m[%s]\033[0m \033[37mRename \"%s\" to:\033[0m " "$(date '+%H:%M:%S')" "$current_title" >&2
        
        # Try different methods to get input working in non-TTY environments
        if [[ -t 0 ]]; then
            # If stdin is already a terminal, just read normally
            read -e -r new_title
        elif [[ -t 1 ]]; then
            # If stdout is a terminal, try to read from it
            read -e -r new_title <&1
        elif [[ -t /dev/tty ]]; then
            # Try reading directly from /dev/tty
            read -e -r new_title < /dev/tty
        elif command -v zenity >/dev/null 2>&1; then
            # Use zenity for GUI input if available
            new_title=$(zenity --entry --title="Rename Conversation" --text="Rename \"$current_title\" to:" --entry-text="$current_title" 2>/dev/null)
        elif command -v dialog >/dev/null 2>&1; then
            # Use dialog for TUI input if available
            new_title=$(dialog --inputbox "Rename \"$current_title\" to:" 0 0 "$current_title" 3>&1 1>&2 2>&3 3>&-)
        elif [[ -f "/tmp/.forge_input" ]]; then
            # Read from temporary file
            new_title=$(cat "/tmp/.forge_input" 2>/dev/null)
            rm -f "/tmp/.forge_input"
        else
            # Fallback - show instructions
            cat >&2 << 'EOF'
⏺ [$(date '+%H:%M:%S')] ERROR: No interactive terminal available for input.

Available methods to provide input:
1. Command line argument: forge conversation rename <id> "new title"
2. Environment variable: FORGE_INPUT="new title" forge conversation rename <id>
3. Temporary file: echo "new title" > /tmp/.forge_input; forge conversation rename <id>
4. GUI dialog (if available): install zenity for graphical input
EOF
            return 1
        fi
    fi
    
    # Handle empty input (just Enter) - use current title as default
    if [[ -z "$new_title" ]]; then
        new_title="$current_title"
    fi
    
    # Handle Ctrl+C (interrupt) - check if read was interrupted
    # Note: $? after read will be 0 for successful read, 1 for EOF/Ctrl+C
    if [[ $? -ne 0 && -z "$new_title" ]]; then
        echo
        _forge_log info "Rename cancelled"
        return 0
    fi
    
    # Validate new title
    if [[ -z "${new_title// }" ]]; then
        _forge_log error "Title cannot be empty"
        return 1
    fi
    
    if [[ "$new_title" == "$current_title" ]]; then
        _forge_log info "Title unchanged"
        return 0
    fi
    
    # Execute rename command
    _forge_log info "Renaming conversation \033[1m${conversation_id}\033[0m"
    local rename_output
    rename_output=$($_FORGE_BIN conversation rename "$conversation_id" "$new_title" 2>&1)
    local rename_exit_code=$?
    
    if [[ $rename_exit_code -eq 0 ]]; then
        _forge_log success "Conversation \033[1m${conversation_id}\033[0m renamed to \"\033[1m${new_title}\033[0m\""
        # Update terminal title after successful rename
        _forge_update_terminal_title
    else
        _forge_log error "Failed to rename conversation: $rename_output"
    fi
}

# Helper function to clone and switch to conversation
function _forge_clone_and_switch() {
    local clone_target="$1"
    
    # Store original conversation ID to check if we're cloning current conversation
    local original_conversation_id="$_FORGE_CONVERSATION_ID"
    
    # Execute clone command
    _forge_log info "Cloning conversation \033[1m${clone_target}\033[0m"
    local clone_output
    clone_output=$($_FORGE_BIN conversation clone "$clone_target" 2>&1)
    local clone_exit_code=$?
    
    if [[ $clone_exit_code -eq 0 ]]; then
        # Extract new conversation ID from output
        local new_id=$(echo "$clone_output" | grep -oE '[a-f0-9-]{36}' | tail -1)
        
        if [[ -n "$new_id" ]]; then
            # Set as active conversation
            _FORGE_CONVERSATION_ID="$new_id"
            
            _forge_log success "└─ Switched to conversation \033[1m${new_id}\033[0m"
            
            # Update terminal title after cloning and switching
            _forge_update_terminal_title
            
            # Show content and info only if cloning a different conversation (not current one)
            if [[ "$clone_target" != "$original_conversation_id" ]]; then
                echo
                _forge_exec conversation show "$new_id"
                
                # Show new conversation info
                echo
                _forge_exec conversation info "$new_id"
            fi
        else
            _forge_log error "Failed to extract new conversation ID from clone output"
        fi
    else
        _forge_log error "Failed to clone conversation: $clone_output"
    fi
}
