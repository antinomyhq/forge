# Execute forge commands interactively with TTY redirection
# Same as _forge_exec but connects stdin/stdout to /dev/tty so that
# interactive prompts (rustyline, fzf, etc.) work correctly when forge
# is launched from a key binding context. Fish key binding functions
# do not have direct terminal access, so without this redirect any
# readline library would see a non-tty stdin and return EOF immediately.
# Do NOT use inside (command) substitutions - use _forge_exec instead.
# Usage: _forge_exec_interactive <args...>

function _forge_exec_interactive
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

    # Execute with TTY redirection for interactive use
    $cmd $argv </dev/tty >/dev/tty
end
