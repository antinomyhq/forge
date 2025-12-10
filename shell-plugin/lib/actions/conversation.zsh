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

function _forge_delete_with_confirmation() {
    local conversation_id="$1"
    
    # Check if trying to delete current conversation
    if [[ "$conversation_id" == "$_FORGE_CONVERSATION_ID" ]]; then
        _forge_log error "Cannot delete currently active conversation"
        return 1
    fi
    
    # Load read-from-minibuffer function
    autoload -Uz read-from-minibuffer
    
    # Show conversation info before asking for confirmation
    echo
    _forge_log info "Conversation to delete:"
    _forge_exec conversation info "$conversation_id"
    echo
    
    # Ask for confirmation
    local REPLY
    read-from-minibuffer "Delete conversation ${conversation_id}? [y/N]: "
    
    if [[ -n "$REPLY" && "$REPLY" =~ ^[Yy]$ ]]; then
        # Execute delete command
        _forge_log info "Deleting conversation \033[1m${conversation_id}\033[0m"
        local delete_output
        delete_output=$($_FORGE_BIN conversation delete "$conversation_id" 2>&1)
        local delete_exit_code=$?
        
        if [[ $delete_exit_code -eq 0 ]]; then
            _forge_log success "└─ Conversation \033[1m${conversation_id}\033[0m deleted successfully"
        else
            _forge_log error "Failed to delete conversation: $delete_output"
        fi
    else
        _forge_log info "Deletion cancelled"
    fi
}

# Action handler: Delete conversation
function _forge_action_delete() {
    local input_text="$1"
    local conversation_id="$input_text"
    
    echo
    
    # Handle explicit conversation ID if provided
    if [[ -z "$conversation_id" ]]; then
        # Get conversations list for fzf selection
        local conversations_output
        conversations_output=$($_FORGE_BIN conversation list --porcelain 2>/dev/null)
        
        if [[ -z "$conversations_output" ]]; then
            _forge_log error "No conversations found"
            _forge_reset
            return 0
        fi
        
        # Create fzf interface similar to :conversation
        local prompt_text="Delete Conversation ❯ "
        local fzf_args=(
            --prompt="$prompt_text"
            --delimiter="$_FORGE_DELIMITER"
            --with-nth="2,3"
            --preview="CLICOLOR_FORCE=1 $_FORGE_BIN conversation info {1}; echo; CLICOLOR_FORCE=1 $_FORGE_BIN conversation show {1}"
            $_FORGE_PREVIEW_WINDOW
        )

        local selected_conversation
        selected_conversation=$(echo "$conversations_output" | _forge_fzf --header-lines=1 "${fzf_args[@]}")
        
        if [[ -n "$selected_conversation" ]]; then
            # Extract conversation ID
            conversation_id=$(echo "$selected_conversation" | sed -E 's/  .*//' | tr -d '\n')
        else
            _forge_reset
            return 0
        fi
    fi
    
    # Execute deletion with confirmation
    _forge_delete_with_confirmation "$conversation_id"
    _forge_reset
}
