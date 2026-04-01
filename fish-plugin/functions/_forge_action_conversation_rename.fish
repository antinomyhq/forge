# Action handler: Rename a conversation (interactive picker or by ID)
# Port of _forge_action_conversation_rename from shell-plugin/lib/actions/conversation.zsh
#
# Usage:
#   :conversation-rename              - Interactive picker, then prompt for name
#   :conversation-rename <id> <name>  - Rename specific conversation directly

function _forge_action_conversation_rename
    set -l input_text ""
    if test (count $argv) -ge 1
        set input_text $argv[1]
    end

    echo

    # If input looks like "<id> <name>", split and rename directly
    if test -n "$input_text"
        # Split on first space: id is before, name is after
        set -l parts (string split -m1 ' ' -- "$input_text")
        set -l conversation_id $parts[1]
        set -l new_name ""
        if test (count $parts) -ge 2
            set new_name $parts[2]
        end

        if test -z "$new_name"
            # Only one arg provided -- not enough
            _forge_log error "Usage: :conversation-rename <id> <name>"
            return 0
        end

        _forge_exec conversation rename "$conversation_id" $new_name
        return 0
    end

    # No args -- show interactive picker
    set -l conversations_output ($_FORGE_BIN conversation list --porcelain 2>/dev/null | string collect)

    if test -z "$conversations_output"
        _forge_log error "No conversations found"
        return 0
    end

    set -l current_id "$_FORGE_CONVERSATION_ID"

    set -l prompt_text "Rename Conversation ❯ "
    set -l fzf_args \
        --prompt="$prompt_text" \
        --delimiter="$_FORGE_DELIMITER" \
        --with-nth="2,3" \
        --preview="CLICOLOR_FORCE=1 $_FORGE_BIN conversation info {1}; echo; CLICOLOR_FORCE=1 $_FORGE_BIN conversation show {1}" \
        $_FORGE_PREVIEW_WINDOW

    if test -n "$current_id"
        set -l index (_forge_find_index "$conversations_output" "$current_id" 1)
        set -a fzf_args --bind="start:pos($index)"
    end

    set -l selected_conversation (echo "$conversations_output" | _forge_fzf --header-lines=1 $fzf_args)

    if test -n "$selected_conversation"
        set -l conversation_id (echo "$selected_conversation" | sed -E 's/  .*//' | tr -d '\n')

        # Prompt for new name
        read -P "Enter new name: " new_name </dev/tty

        if test -n "$new_name"
            _forge_exec conversation rename "$conversation_id" $new_name
        else
            _forge_log error "No name provided, rename cancelled"
        end
    end
end
