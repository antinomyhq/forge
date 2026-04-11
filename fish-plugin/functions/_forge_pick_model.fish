# Helper: Open an fzf model picker and print the raw selected line.
# Port of _forge_pick_model from zsh.
#
# Model list columns (from `forge list models --porcelain`):
#   1:model_id  2:model_name  3:provider(display)  4:provider_id(raw)  5:context  6:tools  7:image
# The picker hides model_id (field 1) and provider_id (field 4) via --with-nth.
#
# Arguments:
#   $argv[1]  prompt_text      - fzf prompt label (e.g. "Model > ")
#   $argv[2]  current_model    - model_id to pre-position the cursor on (may be empty)
#   $argv[3]  input_text       - optional pre-fill query for fzf
#   $argv[4]  current_provider - provider value to disambiguate when model names collide (may be empty)
#   $argv[5]  provider_field   - which porcelain field to match the provider against
#                                (3 for display name, 4 for raw id)
#
# Outputs the raw selected line to stdout, or nothing if cancelled.

function _forge_pick_model
    set -l prompt_text $argv[1]
    set -l current_model $argv[2]
    set -l input_text ""
    set -l current_provider ""
    set -l provider_field ""

    if test (count $argv) -ge 3
        set input_text $argv[3]
    end
    if test (count $argv) -ge 4
        set current_provider $argv[4]
    end
    if test (count $argv) -ge 5
        set provider_field $argv[5]
    end

    set -l output ($_FORGE_BIN list models --porcelain 2>/dev/null | string collect)

    if test -z "$output"
        return 1
    end

    set -l fzf_args \
        --delimiter="$_FORGE_DELIMITER" \
        --prompt="$prompt_text" \
        --with-nth="2,3,5.."

    if test -n "$input_text"
        set fzf_args $fzf_args --query="$input_text"
    end

    if test -n "$current_model"
        # Match on both model_id (field 1) and provider to disambiguate
        # when the same model name exists across multiple providers
        set -l index
        if test -n "$current_provider"; and test -n "$provider_field"
            set index (_forge_find_index "$output" "$current_model" 1 "$provider_field" "$current_provider")
        else
            set index (_forge_find_index "$output" "$current_model" 1)
        end
        set fzf_args $fzf_args --bind="start:pos($index)"
    end

    echo "$output" | _forge_fzf --header-lines=1 $fzf_args
end
