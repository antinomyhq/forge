# Action handler: Login to provider
# Port of _forge_action_login from zsh.
# Interactive provider login with fzf selection.
# Usage: _forge_action_login [input_text]

function _forge_action_login
    set -l input_text $argv[1]
    echo
    # Pass input_text as query parameter for fuzzy search
    set -l selected (_forge_select_provider "" "" "" "$input_text")
    if test -n "$selected"
        # Extract the second field (provider ID)
        set -l provider (echo "$selected" | awk '{print $2}')
        _forge_exec_interactive provider login "$provider"
    end
end
