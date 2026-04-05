# Action handler: Select model (across all configured providers)
# Port of _forge_action_model from zsh.
# When the selected model belongs to a different provider, switches it first.
# Usage: _forge_action_model [input_text]

function _forge_action_model
    set -l input_text $argv[1]

    echo
    set -l current_model ($_FORGE_BIN config get model 2>/dev/null)
    # config get provider returns the display name (e.g. "OpenAI"),
    # which corresponds to porcelain field 3 (provider display)
    set -l current_provider ($_FORGE_BIN config get provider 2>/dev/null)
    set -l selected (_forge_pick_model "Model > " "$current_model" "$input_text" "$current_provider" 3)

    if test -n "$selected"
        # Field 1 = model_id (raw), field 3 = provider display name,
        # field 4 = provider_id (raw, for config set)
        set -l model_id (echo "$selected" | awk -F '  +' '{print $1}' | string trim)
        set -l provider_display (echo "$selected" | awk -F '  +' '{print $3}' | string trim)
        set -l provider_id (echo "$selected" | awk -F '  +' '{print $4}' | string trim)

        # Switch provider first if it differs from the current one
        # current_provider (fetched above) is the display name, compare against that
        if test -n "$provider_display"; and test "$provider_display" != "$current_provider"
            _forge_exec_interactive config set provider "$provider_id" --model "$model_id"
            return
        end
        _forge_exec config set model "$model_id"
    end
end
