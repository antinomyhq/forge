# Start background sync job for current workspace
# Checks if sync is enabled (FORGE_SYNC_ENABLED, default true), verifies the
# workspace is indexed, then runs sync in background with output suppressed.
# Usage: _forge_start_background_sync

function _forge_start_background_sync
    # Check if sync is enabled (default to true if not set)
    set -l sync_enabled "true"
    if set -q FORGE_SYNC_ENABLED; and test -n "$FORGE_SYNC_ENABLED"
        set sync_enabled "$FORGE_SYNC_ENABLED"
    end

    if test "$sync_enabled" != "true"
        return 0
    end

    # Get canonical workspace path
    set -l workspace_path (pwd -P)

    # Check if workspace is indexed before attempting sync
    if not _forge_is_workspace_indexed "$workspace_path"
        return 0
    end

    # Run sync in background with all output suppressed
    # Redirect stdin to /dev/null to prevent hanging if sync tries to read input
    $_FORGE_BIN workspace sync "$workspace_path" >/dev/null 2>&1 </dev/null &
    disown
end
