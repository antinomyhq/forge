# Action handler: Clone conversation
# Port of _forge_action_clone from shell-plugin/lib/actions/conversation.zsh
#
# If a conversation ID is provided, clones it directly.
# Otherwise, opens an interactive fzf picker to select a conversation to clone.
#
# Usage: _forge_action_clone [conversation_id]

function _forge_action_clone
    set -l input_text ""
    if test (count $argv) -ge 1
        set input_text $argv[1]
    end
    set -l clone_target "$input_text"

    echo

    # Handle explicit clone target if provided
    if test -n "$clone_target"
        _forge_clone_and_switch "$clone_target"
        return 0
    end

    # Get conversations list for fzf selection
    set -l conversations_output ($_FORGE_BIN conversation list --porcelain 2>/dev/null | string collect)

    if test -z "$conversations_output"
        _forge_log error "No conversations found"
        return 0
    end

    # Get current conversation ID if set
    set -l current_id "$_FORGE_CONVERSATION_ID"

    # Create fzf interface similar to :conversation
    set -l prompt_text "Clone Conversation ❯ "
    set -l fzf_args \
        --prompt="$prompt_text" \
        --delimiter="$_FORGE_DELIMITER" \
        --with-nth="2,3" \
        --preview="CLICOLOR_FORCE=1 $_FORGE_BIN conversation info {1}; echo; CLICOLOR_FORCE=1 $_FORGE_BIN conversation show {1}" \
        $_FORGE_PREVIEW_WINDOW

    # Position cursor on current conversation if available
    if test -n "$current_id"
        set -l index (_forge_find_index "$conversations_output" "$current_id")
        set -a fzf_args --bind="start:pos($index)"
    end

    set -l selected_conversation (echo "$conversations_output" | _forge_fzf --header-lines=1 $fzf_args)

    if test -n "$selected_conversation"
        # Extract conversation ID
        set -l conversation_id (echo "$selected_conversation" | sed -E 's/  .*//' | tr -d '\n')
        _forge_clone_and_switch "$conversation_id"
    end
end
