#!/usr/bin/env zsh

# Git integration action handlers

# Action handler: List and switch git worktrees using fzf
# Usage: :worktree
# Displays all worktrees with fzf and changes the shell's working directory to the selected one
function _forge_action_worktree() {
    echo

    # Get list of worktrees from git
    local worktrees_output
    worktrees_output=$(git worktree list 2>/dev/null)

    if [[ -z "$worktrees_output" ]]; then
        _forge_log error "No git worktrees found or not inside a git repository"
        return 0
    fi

    # Count worktrees; if only one exists, nothing to switch to
    local worktree_count
    worktree_count=$(echo "$worktrees_output" | wc -l | tr -d ' ')

    if [[ "$worktree_count" -lt 1 ]]; then
        _forge_log error "No worktrees available"
        return 0
    fi

    # Get current worktree path for pre-selection
    local current_path
    current_path=$(git rev-parse --show-toplevel 2>/dev/null)

    # Use fzf to select a worktree
    # git worktree list format: <path>  <commit>  [<branch>]
    local fzf_args=(
        --prompt="Worktree ❯ "
        --delimiter="$_FORGE_DELIMITER"
        --preview="ls -la {1}"
        $_FORGE_PREVIEW_WINDOW
    )

    # Pre-select the current worktree
    local index=$(_forge_find_index "$worktrees_output" "$current_path" 1)
    fzf_args+=(--bind="start:pos($index)")

    local selected
    selected=$(echo "$worktrees_output" | _forge_fzf "${fzf_args[@]}")

    if [[ -n "$selected" ]]; then
        # Extract the path (first field)
        local worktree_path
        worktree_path=$(echo "$selected" | awk '{print $1}')

        if [[ -d "$worktree_path" ]]; then
            # Change directory to the selected worktree directly
            builtin cd "$worktree_path"
            _forge_log success "Switched to worktree \033[1m${worktree_path}\033[0m"
        else
            _forge_log error "Worktree path does not exist: $worktree_path"
        fi
    fi
}

# Action handler: Create a new git worktree
# Usage: :worktree-create <branch-name>
# Executes `git worktree add` directly and reports the result
function _forge_action_worktree_create() {
    local branch="$1"

    echo

    if [[ -z "$branch" ]]; then
        _forge_log error "Usage: :worktree-create <branch-name>"
        return 0
    fi

    # Derive a filesystem-safe directory name from the branch name:
    # take the last path segment (e.g. "feature/foo" -> "foo") and strip unsafe chars
    local dir_name="${branch##*/}"
    dir_name="${dir_name//[^a-zA-Z0-9._-]/-}"

    # Place the worktree at a sibling directory of the repo root
    local repo_root
    repo_root=$(git rev-parse --show-toplevel 2>/dev/null)

    if [[ -z "$repo_root" ]]; then
        _forge_log error "Not inside a git repository"
        return 0
    fi

    local worktree_path="${repo_root%/*}/${dir_name}"

    # Execute the worktree creation directly
    if git worktree add "$worktree_path" "$branch" 2>&1; then
        _forge_log success "Created worktree \033[1m${worktree_path}\033[0m for branch \033[1m${branch}\033[0m"
        builtin cd "$worktree_path"
        _forge_log success "Switched to \033[1m${worktree_path}\033[0m"
    fi
}

# Action handler: Delete a git worktree
# Usage: :worktree-delete [path]
# Without a path, opens fzf to pick a linked worktree to remove.
# Places a ready-to-run `git worktree remove` command in the buffer for review before execution
function _forge_action_worktree_delete() {
    local target="$1"

    echo

    if [[ -n "$target" ]]; then
        # Path supplied directly - put removal command in buffer
        BUFFER="git worktree remove ${(q)target}"
        CURSOR=${#BUFFER}
        zle reset-prompt
        return 0
    fi

    # No path supplied - use fzf to pick from linked worktrees
    local worktrees_output
    worktrees_output=$(git worktree list 2>/dev/null)

    if [[ -z "$worktrees_output" ]]; then
        _forge_log error "No git worktrees found or not inside a git repository"
        return 0
    fi

    # Exclude the main worktree (first line) - it cannot be removed with `git worktree remove`
    local linked_worktrees
    linked_worktrees=$(echo "$worktrees_output" | tail -n +2)

    if [[ -z "$linked_worktrees" ]]; then
        _forge_log error "No linked worktrees to remove (only the main worktree exists)"
        return 0
    fi

    local fzf_args=(
        --prompt="Remove Worktree ❯ "
        --delimiter="$_FORGE_DELIMITER"
        --preview="ls -la {1}"
        $_FORGE_PREVIEW_WINDOW
    )

    local selected
    selected=$(echo "$linked_worktrees" | _forge_fzf "${fzf_args[@]}")

    if [[ -n "$selected" ]]; then
        local worktree_path
        worktree_path=$(echo "$selected" | awk '{print $1}')

        BUFFER="git worktree remove ${(q)worktree_path}"
        CURSOR=${#BUFFER}
        zle reset-prompt
    fi
}

# Action handler: Directly commit changes with AI-generated message
# Usage: :commit [additional context]
# Note: This action clears the buffer after execution
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
    _forge_reset
}


# Action handler: Previews AI-generated commit message 
# Usage: :commit-preview [additional context]
function _forge_action_commit_preview() {
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
            BUFFER="git commit -am ${(qq)commit_message}"
        else
            # Staged changes exist: commit only what's staged
            BUFFER="git commit -m ${(qq)commit_message}"
        fi
        # Move cursor to end of buffer for immediate execution
        CURSOR=${#BUFFER}
        # Refresh display to show the new command
        zle reset-prompt
    else
        _forge_reset
    fi
}
