#!/usr/bin/env zsh

# Git integration action handlers

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
        commit_message=$(FORCE_COLOR=true CLICOLOR_FORCE=1 $_FORGE_BIN commit --max-diff "$_FORGE_MAX_COMMIT_DIFF" $additional_context)
    else
        commit_message=$(FORCE_COLOR=true CLICOLOR_FORCE=1 $_FORGE_BIN commit --max-diff "$_FORGE_MAX_COMMIT_DIFF")
    fi
}
