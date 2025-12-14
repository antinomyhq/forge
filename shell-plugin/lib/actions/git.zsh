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
