#!/usr/bin/env zsh

# Git integration action handlers

# Action handler: Create new git worktree with directory structure
# Usage: :worktree <branch-name> or :sandbox <branch-name>
function _forge_action_worktree() {
    local branch_name="$1"
    
    # Validate branch name parameter
    if [[ -z "$branch_name" ]]; then
        _forge_log error "Branch name is required. Usage: :worktree <branch-name>"
        _forge_reset
        return 1
    fi
    
    # Check if we're in a git repository
    if ! git rev-parse --git-dir >/dev/null 2>&1; then
        _forge_log error "Not in a git repository"
        _forge_reset
        return 1
    fi
    
    # Check if branch already exists
    if git show-ref --verify --quiet "refs/heads/$branch_name"; then
        _forge_log error "Branch '\033[1m${branch_name}\033[0m' already exists"
        _forge_reset
        return 1
    fi
    
    # Initialize worktree path
    local worktree_path="../$branch_name"
    
    # Handle directory structure for branches like feature/xxx, fix/xxx, etc.
    if [[ "$branch_name" =~ ^([^/]+)/([^/]+)$ ]]; then
        local category="${match[1]}"
        local actual_branch="${match[2]}"
        
        # Create category directory if it doesn't exist
        mkdir -p "../$category"
        worktree_path="../$category/$actual_branch"
        
        # Re-validate worktree path after creating directory structure
        if [[ -d "$worktree_path" ]]; then
            _forge_log error "Directory '\033[1m${worktree_path}\033[0m' already exists"
            _forge_reset
            return 1
        fi
    else
        # For simple branch names, check if directory already exists
        if [[ -d "$worktree_path" ]]; then
            _forge_log error "Directory '\033[1m${worktree_path}\033[0m' already exists"
            _forge_reset
            return 1
        fi
    fi
    
    # Validate branch name format (basic git branch name validation)
    if [[ "$branch_name" =~ [^a-zA-Z0-9/_-] ]]; then
        _forge_log error "Invalid branch name format. Only alphanumeric characters, '/', '_', and '-' are allowed"
        _forge_reset
        return 1
    fi
    
    # Execute git worktree creation
    _forge_log info "Creating new worktree: \033[1m${worktree_path}\033[0m"
    
    if git worktree add "$worktree_path" -b "$branch_name"; then
        _forge_log success "Worktree created successfully for branch '\033[1m${branch_name}\033[0m'"
        
        # Change to the new worktree directory
        if cd "$worktree_path"; then
            _forge_log success "Switched to worktree: \033[1m$(pwd)\033[0m"
        else
            _forge_log error "Failed to change to worktree directory"
            _forge_reset
            return 1
        fi
    else
        _forge_log error "Failed to create worktree"
        _forge_reset
        return 1
    fi
    
    _forge_reset
}

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
