#!/usr/bin/env zsh

# Authentication action handlers

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
}

# Action handler: Logout from provider
function _forge_action_logout() {
    echo
    local selected
    selected=$(_forge_select_provider "\[yes\]")
    if [[ -n "$selected" ]]; then
        # Extract the second field (provider ID)
        local provider=$(echo "$selected" | awk '{print $2}')
        _forge_exec provider logout "$provider"
    fi
}
