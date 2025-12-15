#!/usr/bin/env zsh

# Doctor action handler for forge environment diagnostics

# Action handler: Run forge environment diagnostics
# Executes the forge binary's zsh doctor command
function _forge_action_doctor() {
    echo
    
    # Execute forge zsh doctor command
    $_FORGE_BIN zsh doctor
    
    _forge_reset
}
