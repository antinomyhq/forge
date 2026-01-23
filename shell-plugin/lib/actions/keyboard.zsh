#!/usr/bin/env zsh

# Keyboard action handler for ZSH keyboard shortcuts

# Action handler: Display ZSH keyboard shortcuts
# Executes the forge binary's zsh keyboard command
function _forge_action_keyboard() {
    echo
    
    # Execute forge zsh keyboard command
    $_FORGE_BIN zsh keyboard
}
