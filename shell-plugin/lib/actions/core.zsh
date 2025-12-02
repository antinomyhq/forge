#!/usr/bin/env zsh

# Core action handlers for basic forge operations

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
