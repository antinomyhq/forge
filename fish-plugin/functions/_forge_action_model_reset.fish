# Action handler: Reset session model and provider to defaults.
# Port of _forge_action_model_reset from zsh.
# Clears both _FORGE_SESSION_MODEL and _FORGE_SESSION_PROVIDER,
# reverting to global config for subsequent forge invocations.
# Usage: _forge_action_model_reset

function _forge_action_model_reset
    echo

    if test -z "$_FORGE_SESSION_MODEL"; and test -z "$_FORGE_SESSION_PROVIDER"
        _forge_log info "Session model already cleared (using global config)"
        return 0
    end

    set -g _FORGE_SESSION_MODEL ""
    set -g _FORGE_SESSION_PROVIDER ""

    _forge_log success "Session model reset to global config"
end
