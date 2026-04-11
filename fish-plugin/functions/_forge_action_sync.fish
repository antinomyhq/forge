# Action handler: Sync workspace for codebase search
# --init initializes the workspace first if it has not been set up yet
# Usage: _forge_action_sync

function _forge_action_sync
    echo
    # Execute sync with stdin redirected to prevent hanging
    # Sync doesn't need interactive input, so close stdin immediately
    _forge_exec workspace sync --init </dev/null
end
