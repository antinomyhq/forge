# Action handler: Initialize workspace for codebase search
# Usage: _forge_action_sync_init

function _forge_action_sync_init
    echo
    _forge_exec workspace init </dev/null
end
