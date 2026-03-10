#!/usr/bin/env zsh

# Git integration action handlers

# Action handler: List and switch git worktrees using fzf
# Usage: :worktree
# Delegates listing to `forge worktree list` (tab-separated branch<TAB>path),
# displays via fzf, then cd into the selected worktree.
function _forge_action_worktree() {
    echo

    # Fetch porcelain list from CLI: each line is "branch\tpath"
    local worktrees_raw
    worktrees_raw=$($_FORGE_BIN worktree list 2>/dev/null)

    if [[ -z "$worktrees_raw" ]]; then
        _forge_log error "No git worktrees found or not inside a git repository"
        return 0
    fi

    # Build space-padded display table with a BRANCH / DIRECTORY header.
    # forge worktree list emits: branch<TAB>path
    local worktrees_output
    worktrees_output=$(echo "$worktrees_raw" | awk -F'\t' '
        BEGIN { maxlen = 6 }
        {
            branches[NR] = $1
            paths[NR] = $2
            if (length($1) > maxlen) maxlen = length($1)
        }
        END {
            printf "%-" maxlen "s  %s\n", "BRANCH", "DIRECTORY"
            for (i = 1; i <= NR; i++) {
                printf "%-" maxlen "s  %s\n", branches[i], paths[i]
            }
        }
    ')

    # Get current worktree path for pre-selection
    local current_path
    current_path=$(git rev-parse --show-toplevel 2>/dev/null)

    local fzf_args=(
        --prompt="Worktree ❯ "
        --delimiter="$_FORGE_DELIMITER"
        --with-nth=1,2
        --preview="ls -la {2}"
        "$_FORGE_PREVIEW_WINDOW"
        --header-lines=1
    )

    local index=$(_forge_find_index "$worktrees_output" "$current_path" 2)
    fzf_args+=(--bind="start:pos($index)")

    local selected
    selected=$(echo "$worktrees_output" | _forge_fzf "${fzf_args[@]}")

    if [[ -n "$selected" ]]; then
        local worktree_path
        worktree_path=$(echo "$selected" | sed 's/^[^ ]*  *//')

        if [[ -d "$worktree_path" ]]; then
            builtin cd "$worktree_path"
            _forge_log success "Switched to worktree \033[1m${worktree_path}\033[0m"
        else
            _forge_log error "Worktree path does not exist: $worktree_path"
        fi
    fi
}

# Action handler: Create a new git worktree
# Usage: :worktree-create <branch-name>
# Delegates to `forge worktree create <branch>` which prints the new path on
# stdout; the shell then cd's into it.
function _forge_action_worktree_create() {
    local branch="$1"

    echo

    if [[ -z "$branch" ]]; then
        _forge_log error "Usage: :worktree-create <branch-name>"
        return 0
    fi

    local worktree_path
    worktree_path=$($_FORGE_BIN worktree create "$branch" 2>&1)
    local exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        _forge_log error "Failed to create worktree: $worktree_path"
        return 0
    fi

    _forge_log success "Created worktree \033[1m${worktree_path}\033[0m for branch \033[1m${branch}\033[0m"
    builtin cd "$worktree_path"
    _forge_log success "Switched to \033[1m${worktree_path}\033[0m"
}

# Action handler: Delete a git worktree
# Usage: :worktree-delete [path]
# Without a path, opens fzf (populated via `forge worktree list`) to pick a
# linked worktree; then delegates removal to `forge worktree delete <path>`.
function _forge_action_worktree_delete() {
    local target="$1"

    echo

    if [[ -n "$target" ]]; then
        # Path supplied directly - delegate to CLI
        $_FORGE_BIN worktree delete "$target"
        return 0
    fi

    # No path supplied - fetch linked worktrees (skip the main/first entry)
    local worktrees_raw
    worktrees_raw=$($_FORGE_BIN worktree list 2>/dev/null | tail -n +2)

    if [[ -z "$worktrees_raw" ]]; then
        _forge_log error "No linked worktrees to remove (only the main worktree exists)"
        return 0
    fi

    # Build space-padded display table
    local linked_worktrees
    linked_worktrees=$(echo "$worktrees_raw" | awk -F'\t' '
        BEGIN { maxlen = 6 }
        {
            branches[NR] = $1
            paths[NR] = $2
            if (length($1) > maxlen) maxlen = length($1)
        }
        END {
            printf "%-" maxlen "s  %s\n", "BRANCH", "DIRECTORY"
            for (i = 1; i <= NR; i++) {
                printf "%-" maxlen "s  %s\n", branches[i], paths[i]
            }
        }
    ')

    local fzf_args=(
        --prompt="Remove Worktree ❯ "
        --delimiter="$_FORGE_DELIMITER"
        --with-nth=1,2
        --preview="ls -la {2}"
        "$_FORGE_PREVIEW_WINDOW"
        --header-lines=1
    )

    local selected
    selected=$(echo "$linked_worktrees" | _forge_fzf "${fzf_args[@]}")

    if [[ -n "$selected" ]]; then
        local worktree_path
        worktree_path=$(echo "$selected" | sed 's/^[^ ]*  *//')
        $_FORGE_BIN worktree delete "$worktree_path"
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
