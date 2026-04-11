# Action handler: List/switch conversations
# Port of _forge_action_conversation from shell-plugin/lib/actions/conversation.zsh
#
# Features:
#   :conversation          - List and switch conversations (with fzf)
#   :conversation <id>     - Switch to specific conversation by ID
#   :conversation -        - Toggle between current and previous conversation (like cd -)
#
# Usage: _forge_action_conversation [input_text]

function _forge_action_conversation
    set -l input_text ""
    if test (count $argv) -ge 1
        set input_text $argv[1]
    end

    echo

    # Handle toggling to previous conversation (like cd -)
    if test "$input_text" = -
        # Check if there's a previous conversation
        if test -z "$_FORGE_PREVIOUS_CONVERSATION_ID"
            # No previous conversation tracked, fall through to list
            set input_text ""
        else
            # Swap current and previous
            set -l temp "$_FORGE_CONVERSATION_ID"
            set -g _FORGE_CONVERSATION_ID "$_FORGE_PREVIOUS_CONVERSATION_ID"
            set -g _FORGE_PREVIOUS_CONVERSATION_ID "$temp"

            # Show conversation content
            echo
            _forge_exec conversation show "$_FORGE_CONVERSATION_ID"

            # Show conversation info
            _forge_exec conversation info "$_FORGE_CONVERSATION_ID"

            # Print log about conversation switching
            _forge_log success "Switched to conversation "(set_color --bold)"$_FORGE_CONVERSATION_ID"(set_color normal)

            return 0
        end
    end

    # If an ID is provided directly, use it
    if test -n "$input_text"
        set -l conversation_id "$input_text"

        # Switch to conversation and track in history
        _forge_switch_conversation "$conversation_id"

        # Show conversation content
        echo
        _forge_exec conversation show "$conversation_id"

        # Show conversation info
        _forge_exec conversation info "$conversation_id"

        # Print log about conversation switching
        _forge_log success "Switched to conversation "(set_color --bold)"$conversation_id"(set_color normal)

        return 0
    end

    # Get conversations list
    set -l conversations_output ($_FORGE_BIN conversation list --porcelain 2>/dev/null | string collect)

    if test -n "$conversations_output"
        # Get current conversation ID if set
        set -l current_id "$_FORGE_CONVERSATION_ID"

        # Create prompt with current conversation
        set -l prompt_text "Conversation ❯ "
        set -l fzf_args \
            --prompt="$prompt_text" \
            --delimiter="$_FORGE_DELIMITER" \
            --with-nth="2,3" \
            --preview="CLICOLOR_FORCE=1 $_FORGE_BIN conversation info {1}; echo; CLICOLOR_FORCE=1 $_FORGE_BIN conversation show {1}" \
            $_FORGE_PREVIEW_WINDOW

        # If there's a current conversation, position cursor on it
        if test -n "$current_id"
            set -l index (_forge_find_index "$conversations_output" "$current_id" 1)
            set -a fzf_args --bind="start:pos($index)"
        end

        # Use fzf with preview showing the last message from the conversation
        set -l selected_conversation (echo "$conversations_output" | _forge_fzf --header-lines=1 $fzf_args)

        if test -n "$selected_conversation"
            # Extract the first field (UUID) - everything before the first multi-space delimiter
            set -l conversation_id (echo "$selected_conversation" | sed -E 's/  .*//' | tr -d '\n')

            # Switch to conversation and track in history
            _forge_switch_conversation "$conversation_id"

            # Show conversation content
            echo
            _forge_exec conversation show "$conversation_id"

            # Show conversation info
            _forge_exec conversation info "$conversation_id"

            # Print log about conversation switching
            _forge_log success "Switched to conversation "(set_color --bold)"$conversation_id"(set_color normal)
        end
    else
        _forge_log error "No conversations found"
    end
end
