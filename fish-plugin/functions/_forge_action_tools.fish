# Action handler: Show tools for active agent
# Usage: _forge_action_tools

function _forge_action_tools
    echo
    # Ensure FORGE_ACTIVE_AGENT always has a value, default to "forge"
    set -l agent_id "forge"
    if test -n "$_FORGE_ACTIVE_AGENT"
        set agent_id "$_FORGE_ACTIVE_AGENT"
    end
    _forge_exec list tools "$agent_id"
end
