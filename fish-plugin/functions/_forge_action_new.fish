# Action handler: Start a new conversation
# Port of _forge_action_new from shell-plugin/lib/actions/core.zsh
#
# Clears the current conversation, resets the active agent, and optionally
# starts a new conversation with the given input text.
#
# Usage: _forge_action_new [input_text]

function _forge_action_new
    set -l input_text ""
    if test (count $argv) -ge 1
        set input_text $argv[1]
    end

    # Clear conversation and save as previous (like cd -)
    _forge_clear_conversation
    set -g _FORGE_ACTIVE_AGENT forge

    echo

    # If input_text is provided, send it to the new conversation
    if test -n "$input_text"
        # Generate new conversation ID and switch to it
        set -l new_id ($_FORGE_BIN conversation new)
        _forge_switch_conversation "$new_id"

        # Execute the forge command with the input text
        _forge_exec_interactive -p "$input_text" --cid "$_FORGE_CONVERSATION_ID"

        # Start background sync job if enabled and not already running
        _forge_start_background_sync
        # Start background update check
        _forge_start_background_update
    else
        # Only show banner if no input text (starting fresh conversation)
        _forge_exec banner
    end
end
