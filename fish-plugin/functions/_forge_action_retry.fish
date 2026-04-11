# Action handler: Retry last message
# Port of _forge_action_retry from shell-plugin/lib/actions/core.zsh
#
# Retries the last message in the current conversation.
#
# Usage: _forge_action_retry

function _forge_action_retry
    _forge_handle_conversation_command retry
end
