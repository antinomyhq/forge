# Execute forge commands consistently
# Builds command array with forge binary, --agent flag, and passes
# session model/provider as local exports.
# Usage: _forge_exec <args...>

function _forge_exec
    # Determine active agent, default to "forge"
    set -l agent_id "forge"
    if test -n "$_FORGE_ACTIVE_AGENT"
        set agent_id "$_FORGE_ACTIVE_AGENT"
    end

    # Build command array
    set -l cmd $_FORGE_BIN --agent "$agent_id"

    # Export session model/provider if set
    if test -n "$_FORGE_SESSION_MODEL"
        set -lx FORGE_SESSION__MODEL_ID "$_FORGE_SESSION_MODEL"
    end
    if test -n "$_FORGE_SESSION_PROVIDER"
        set -lx FORGE_SESSION__PROVIDER_ID "$_FORGE_SESSION_PROVIDER"
    end

    # Execute with all arguments passed through
    $cmd $argv
end
