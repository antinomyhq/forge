# Action handler: Rename current conversation
# Port of _forge_action_rename from shell-plugin/lib/actions/conversation.zsh
#
# Renames the current active conversation. Requires a name argument.
#
# Usage: _forge_action_rename <name>

function _forge_action_rename
    set -l input_text ""
    if test (count $argv) -ge 1
        set input_text $argv[1]
    end

    echo

    if test -z "$_FORGE_CONVERSATION_ID"
        _forge_log error "No active conversation. Start a conversation first or use :conversation to select one"
        return 0
    end

    if test -z "$input_text"
        _forge_log error "Usage: :rename <name>"
        return 0
    end

    _forge_exec conversation rename "$_FORGE_CONVERSATION_ID" $input_text
end
