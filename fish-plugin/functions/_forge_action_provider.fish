# Action handler: Select LLM provider
# Port of _forge_action_provider from zsh.
# Usage: _forge_action_provider [input_text]

function _forge_action_provider
    set -l input_text $argv[1]
    echo
    # Only show LLM providers (exclude context_engine and other non-LLM types)
    # Pass input_text as query parameter for fuzzy search
    set -l selected (_forge_select_provider "" "" "llm" "$input_text")

    if test -n "$selected"
        # Extract the second field (provider ID) from the selected line
        # Format: "DisplayName  provider_id  host  type  status"
        set -l provider_id (echo "$selected" | awk '{print $2}')
        # Use _forge_exec_interactive because config-set may trigger
        # interactive authentication prompts (rustyline) when the provider
        # is not yet configured.
        _forge_exec_interactive config set provider "$provider_id"
    end
end
