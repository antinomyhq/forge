# Helper function to handle conversation commands that require an active conversation
# Port of _forge_handle_conversation_command from shell-plugin/lib/actions/core.zsh
#
# Verifies that a conversation is active, then executes the given conversation
# subcommand with the conversation ID and any extra arguments.
#
# Usage: _forge_handle_conversation_command <subcommand> [extra_args...]

function _forge_handle_conversation_command
    set -l subcommand $argv[1]
    set -l extra_args $argv[2..]

    echo

    # Check if FORGE_CONVERSATION_ID is set
    if test -z "$_FORGE_CONVERSATION_ID"
        _forge_log error "No active conversation. Start a conversation first or use :conversation to see existing ones"
        return 0
    end

    # Execute the conversation command with conversation ID and any extra arguments
    _forge_exec conversation "$subcommand" "$_FORGE_CONVERSATION_ID" $extra_args
end
