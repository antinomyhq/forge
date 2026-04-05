# Action handler: Show workspace info with sync details
# Usage: _forge_action_sync_info

function _forge_action_sync_info
    echo
    _forge_exec workspace info "."
end
