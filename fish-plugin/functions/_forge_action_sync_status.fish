# Action handler: Show sync status of workspace files
# Usage: _forge_action_sync_status

function _forge_action_sync_status
    echo
    _forge_exec workspace status "."
end
