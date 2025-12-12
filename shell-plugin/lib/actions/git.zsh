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
    
    # Get the git root directory - handle both main repo and worktree cases
    local git_root=$(git rev-parse --show-toplevel)
    if [[ -z "$git_root" ]]; then
        _forge_log error "Failed to determine git repository root"
        _forge_reset
        return 1
    fi
    
    # Check if we're in a worktree and find the main repository
    local main_repo_root="$git_root"
    local git_dir=$(git rev-parse --git-dir)
    
    # If .git is a file (not a directory), we're in a worktree
    # OR if git_dir contains "worktrees", we're in a worktree
    if [[ -f "$git_dir" ]] || [[ "$git_dir" == *"worktrees"* ]]; then
        local commondir="$git_dir/commondir"
        
        if [[ -f "$commondir" ]]; then
            local commondir_content=$(cat "$commondir")
            # commondir contains relative path like "../.." from worktrees directory
            main_repo_root="${git_dir%/*}/$commondir_content"
            # Resolve relative path to absolute
            main_repo_root="$(cd "$main_repo_root" && pwd)"
        else
            # Fallback for file-based .git (older git versions)
            local main_git_dir=$(grep "gitdir:" "$git_dir" | cut -d ' ' -f 2)
            if [[ -n "$main_git_dir" ]]; then
                main_repo_root="${main_git_dir%/.git/worktrees/*}"
                main_repo_root="${main_repo_root%/worktrees/*}"
            fi
        fi
        
        _forge_log info "Detected worktree environment, using main repository: \033[1m${main_repo_root}\033[0m"
    fi
    
    # Check if branch already exists
    if git show-ref --verify --quiet "refs/heads/$branch_name"; then
        _forge_log error "Branch '\033[1m${branch_name}\033[0m' already exists"
        _forge_reset
        return 1
    fi
    
    # Initialize worktree path relative to main repository root, not current directory
    local worktree_path="$main_repo_root/../$branch_name"
    
    # Handle directory structure for branches like feature/xxx, fix/xxx, etc.
    if [[ "$branch_name" =~ ^([^/]+)/([^/]+)$ ]]; then
        local category="${match[1]}"
        local actual_branch="${match[2]}"
        
        # Create category directory if it doesn't exist
        mkdir -p "$main_repo_root/../$category"
        worktree_path="$main_repo_root/../$category/$actual_branch"
        
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
    
    # Execute git worktree creation from the main repository
    _forge_log info "Creating new worktree: \033[1m${worktree_path}\033[0m"
    
    # Change to main repository to ensure proper worktree creation
    local current_dir=$(pwd)
    cd "$main_repo_root"
    
    # Execute git worktree command with English locale and capture output
    local git_output
    git_output=$(LANG=C git worktree add "$worktree_path" -b "$branch_name" 2>&1)
    local git_exit_code=$?
    
    if [[ $git_exit_code -eq 0 ]]; then
        # Process git output lines and format them with proper prefix
        while IFS= read -r line; do
            if [[ -n "$line" ]]; then
                _forge_log info "$line"
            fi
        done <<< "$git_output"
        
        _forge_log success "Worktree created successfully for branch '\033[1m${branch_name}\033[0m'"
        
        # Change to the new worktree directory
        cd "$worktree_path"
        _forge_log success "Switched to worktree: \033[1m$(pwd)\033[0m"
    else
        # Process git error output lines and format them with proper prefix
        while IFS= read -r line; do
            if [[ -n "$line" ]]; then
                _forge_log error "$line"
            fi
        done <<< "$git_output"
        
        _forge_log error "Failed to create worktree"
        # Return to original directory
        cd "$current_dir"
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
