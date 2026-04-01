# Check if a workspace is indexed by forge
# Usage: _forge_is_workspace_indexed <workspace_path>
# Returns: 0 if workspace is indexed, 1 otherwise

function _forge_is_workspace_indexed
    set -l workspace_path $argv[1]
    $_FORGE_BIN workspace info "$workspace_path" >/dev/null 2>&1
    return $status
end
