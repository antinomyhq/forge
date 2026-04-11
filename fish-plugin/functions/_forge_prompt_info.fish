# Model and agent info with token count for the right prompt
# Fully formatted output directly from Rust.
# Returns fish-formatted string ready for use in fish_right_prompt.
# Passes session model/provider as env vars so the rprompt reflects
# the active session override rather than global config.
# Usage: _forge_prompt_info

function _forge_prompt_info
    # Determine forge binary path
    set -l forge_bin "forge"
    if test -n "$_FORGE_BIN"
        set forge_bin "$_FORGE_BIN"
    else if set -q FORGE_BIN; and test -n "$FORGE_BIN"
        set forge_bin "$FORGE_BIN"
    end

    # Export session model/provider if set
    if test -n "$_FORGE_SESSION_MODEL"
        set -lx FORGE_SESSION__MODEL_ID "$_FORGE_SESSION_MODEL"
    end
    if test -n "$_FORGE_SESSION_PROVIDER"
        set -lx FORGE_SESSION__PROVIDER_ID "$_FORGE_SESSION_PROVIDER"
    end

    # Pass conversation ID and active agent as env vars, call forge fish rprompt
    _FORGE_CONVERSATION_ID=$_FORGE_CONVERSATION_ID \
        _FORGE_ACTIVE_AGENT=$_FORGE_ACTIVE_AGENT \
        $forge_bin fish rprompt
end
