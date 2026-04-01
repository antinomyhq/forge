# Action handler: Show environment info
# Port of _forge_action_env from shell-plugin/lib/actions/core.zsh
#
# Displays forge environment configuration.
#
# Usage: _forge_action_env

function _forge_action_env
    echo
    _forge_exec env
end
