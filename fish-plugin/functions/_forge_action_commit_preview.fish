# Action handler: Preview AI-generated commit message
# Port of _forge_action_commit_preview from shell-plugin/lib/actions/git.zsh
#
# Generates an AI commit message in preview mode, then loads the git commit
# command into the command line buffer for the user to review and execute.
# Uses -am flag if no staged changes, -m flag if staged changes exist.
#
# Usage: _forge_action_commit_preview [additional_context]

function _forge_action_commit_preview
    set -l additional_context ""
    if test (count $argv) -ge 1
        set additional_context $argv[1]
    end

    echo

    # Generate AI commit message
    # Force color output even when not connected to TTY
    set -lx FORCE_COLOR true
    set -lx CLICOLOR_FORCE 1
    set -l commit_message
    if test -n "$additional_context"
        set commit_message ($_FORGE_BIN commit --preview --max-diff "$_FORGE_MAX_COMMIT_DIFF" $additional_context | string collect)
    else
        set commit_message ($_FORGE_BIN commit --preview --max-diff "$_FORGE_MAX_COMMIT_DIFF" | string collect)
    end

    # Proceed only if command succeeded
    if test -n "$commit_message"
        # Escape the commit message for safe embedding in the command line
        set -l escaped_message (string escape -- "$commit_message")

        # Check if there are staged changes to determine commit strategy
        if git diff --staged --quiet
            # No staged changes: commit all tracked changes with -a flag
            commandline -r "git commit -am $escaped_message"
        else
            # Staged changes exist: commit only what's staged
            commandline -r "git commit -m $escaped_message"
        end
        commandline -f repaint
    else
        _forge_reset
    end
end
