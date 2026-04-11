# Action handler: Show session info
# Port of _forge_action_info from shell-plugin/lib/actions/core.zsh
#
# Displays forge session information. If a conversation is active,
# passes the --cid flag to show conversation-specific info.
#
# Usage: _forge_action_info

function _forge_action_info
    echo
    if test -n "$_FORGE_CONVERSATION_ID"
        _forge_exec info --cid "$_FORGE_CONVERSATION_ID"
    else
        _forge_exec info
    end
end
