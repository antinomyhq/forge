# Action handler: Directly commit changes with AI-generated message
# Port of _forge_action_commit from shell-plugin/lib/actions/git.zsh
#
# Generates an AI commit message and executes the commit.
# This action clears the buffer after execution (via _forge_reset in dispatcher).
#
# Usage: _forge_action_commit [additional_context]

function _forge_action_commit
    set -l additional_context ""
    if test (count $argv) -ge 1
        set additional_context $argv[1]
    end

    echo

    # Generate AI commit message
    # Force color output even when not connected to TTY
    # FORCE_COLOR: for indicatif spinner colors
    # CLICOLOR_FORCE: for colored crate text colors
    set -lx FORCE_COLOR true
    set -lx CLICOLOR_FORCE 1
    set -l commit_message
    if test -n "$additional_context"
        set commit_message ($_FORGE_BIN commit --max-diff "$_FORGE_MAX_COMMIT_DIFF" $additional_context | string collect)
    else
        set commit_message ($_FORGE_BIN commit --max-diff "$_FORGE_MAX_COMMIT_DIFF" | string collect)
    end

    _forge_reset
end
