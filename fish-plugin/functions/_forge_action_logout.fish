# Action handler: Logout from provider
# Port of _forge_action_logout from zsh.
# Logout from provider (filter by confirmed/yes status).
# Usage: _forge_action_logout [input_text]

function _forge_action_logout
    set -l input_text $argv[1]
    echo
    # Pass input_text as query parameter for fuzzy search
    # Filter to only show confirmed providers (status contains [yes])
    set -l selected (_forge_select_provider '\\[yes\\]' "" "" "$input_text")
    if test -n "$selected"
        # Extract the second field (provider ID)
        set -l provider (echo "$selected" | awk '{print $2}')
        _forge_exec provider logout "$provider"
    end
end
