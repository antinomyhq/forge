# Action handler: Select model for command suggestion generation
# Port of _forge_action_suggest_model from zsh.
# Calls `forge config set suggest <provider_id> <model_id>` on selection.
# Usage: _forge_action_suggest_model [input_text]

function _forge_action_suggest_model
    set -l input_text $argv[1]

    echo
    # config get suggest outputs two lines: provider_id (raw) then model_id
    set -l suggest_output (_forge_exec config get suggest 2>/dev/null | string collect)
    set -l current_suggest_provider (echo "$suggest_output" | head -n 1)
    set -l current_suggest_model (echo "$suggest_output" | tail -n 1)

    # provider_id from config get suggest is the raw id, matching porcelain field 4
    set -l selected (_forge_pick_model "Suggest Model > " "$current_suggest_model" "$input_text" "$current_suggest_provider" 4)

    if test -n "$selected"
        # Field 1 = model_id (raw), field 4 = provider_id (raw)
        set -l model_id (echo "$selected" | awk -F '  +' '{print $1}' | string trim)
        set -l provider_id (echo "$selected" | awk -F '  +' '{print $4}' | string trim)

        _forge_exec config set suggest "$provider_id" "$model_id"
    end
end
