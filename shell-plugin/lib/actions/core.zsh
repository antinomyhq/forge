#!/usr/bin/env zsh

# Core action handlers for basic forge operations

# Ensure a provider is configured before sending a chat message.
# If no provider is active, runs the shell-native provider selection and auth
# flow (_forge_action_provider) instead of letting the Rust CLI fall back to
# its interactive ForgeSelect prompts (which crash on Windows Git Bash).
# Returns 0 if a provider is ready, 1 if the user cancelled or auth failed.
function _forge_ensure_provider() {
    local provider auth_info configured
    provider=$($_FORGE_BIN config get provider --porcelain 2>/dev/null </dev/null)
    if [[ -z "$provider" || "$provider" == "Provider: Not set" ]]; then
        # No provider in config at all — run full provider selection + auth
        _forge_log info "No provider configured. Please select one."
        _forge_action_provider ""
        # Re-check after the selection flow
        provider=$($_FORGE_BIN config get provider --porcelain 2>/dev/null </dev/null)
        if [[ -z "$provider" || "$provider" == "Provider: Not set" ]]; then
            return 1
        fi
        return 0
    fi

    # Provider is in config — check if credentials are valid (not logged out)
    auth_info=$($_FORGE_BIN provider auth-info "$provider" 2>/dev/null </dev/null)
    configured=$(echo "$auth_info" | awk -F= '/^configured=/{print $2}')
    if [[ "$configured" != "yes" ]]; then
        # Provider set but no valid credentials — run auth for this provider
        _forge_log info "Provider '$provider' is not authenticated. Please log in."
        _forge_provider_auth "$provider"
        # Re-check
        auth_info=$($_FORGE_BIN provider auth-info "$provider" 2>/dev/null </dev/null)
        configured=$(echo "$auth_info" | awk -F= '/^configured=/{print $2}')
        if [[ "$configured" != "yes" ]]; then
            return 1
        fi
    fi
    return 0
}

# Action handler: Start a new conversation
function _forge_action_new() {
    local input_text="$1"
    
    # Clear conversation and save as previous (like cd -)
    _forge_clear_conversation
    _FORGE_ACTIVE_AGENT="forge"
    
    echo
    
    # If input_text is provided, send it to the new conversation
    if [[ -n "$input_text" ]]; then
        # Ensure a provider is configured before sending the message
        _forge_ensure_provider || return 0

        # Generate new conversation ID and switch to it
        local new_id=$($_FORGE_BIN conversation new)
        _forge_switch_conversation "$new_id"
        
        # Execute the forge command with the input text
        _forge_exec -p "$input_text" --cid "$_FORGE_CONVERSATION_ID"
        
        # Start background sync job if enabled and not already running
        _forge_start_background_sync
        # Start background update check
        _forge_start_background_update
    else
        # Only show banner if no input text (starting fresh conversation)
        _forge_exec banner
    fi
}

# Action handler: Show session info
function _forge_action_info() {
    echo
    if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
        _forge_exec info --cid "$_FORGE_CONVERSATION_ID"
    else
        _forge_exec info
    fi
}

# Action handler: Show environment info
function _forge_action_env() {
    echo
    _forge_exec env
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

# Helper function to handle conversation commands that require an active conversation
function _forge_handle_conversation_command() {
    local subcommand="$1"
    shift  # Remove first argument, remaining args become extra parameters
    
    echo
    
    # Check if FORGE_CONVERSATION_ID is set
    if [[ -z "$_FORGE_CONVERSATION_ID" ]]; then
        _forge_log error "No active conversation. Start a conversation first or use :list to see existing ones"
        return 0
    fi
    
    # Execute the conversation command with conversation ID and any extra arguments
    _forge_exec conversation "$subcommand" "$_FORGE_CONVERSATION_ID" "$@"
}
