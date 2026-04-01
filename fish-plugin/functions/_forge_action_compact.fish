# Action handler: Compact conversation
# Port of _forge_action_compact from shell-plugin/lib/actions/core.zsh
#
# Compacts the current conversation to reduce token usage.
#
# Usage: _forge_action_compact

function _forge_action_compact
    _forge_handle_conversation_command compact
end
