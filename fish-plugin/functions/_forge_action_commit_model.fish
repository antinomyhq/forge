# Action handler: Select model for commit message generation
# Port of _forge_action_commit_model from zsh.
# Calls `forge config set commit <provider_id> <model_id>` on selection.
# Usage: _forge_action_commit_model [input_text]

function _forge_action_commit_model
    set -l input_text $argv[1]

    echo
    # config get commit outputs two lines: provider_id (raw) then model_id
    set -l commit_output (_forge_exec config get commit 2>/dev/null | string collect)
    set -l current_commit_provider (echo "$commit_output" | head -n 1)
    set -l current_commit_model (echo "$commit_output" | tail -n 1)

    # provider_id from config get commit is the raw id, matching porcelain field 4
    set -l selected (_forge_pick_model "Commit Model > " "$current_commit_model" "$input_text" "$current_commit_provider" 4)

    if test -n "$selected"
        # Field 1 = model_id (raw), field 4 = provider_id (raw)
        set -l model_id (echo "$selected" | awk -F '  +' '{print $1}' | string trim)
        set -l provider_id (echo "$selected" | awk -F '  +' '{print $4}' | string trim)

        _forge_exec config set commit "$provider_id" "$model_id"
    end
end
