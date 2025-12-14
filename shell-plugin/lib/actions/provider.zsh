#!/usr/bin/env zsh

# Provider selection helper

# Helper function to select a provider from the list
# Usage: _forge_select_provider [filter_status] [current_provider] [filter_type]
# Returns: selected provider line (via stdout)
function _forge_select_provider() {
    local filter_status="${1:-}"
    local current_provider="${2:-}"
    local filter_type="${3:-}"
    local output
    
    # Build the command with type filter if specified
    local cmd="$_FORGE_BIN list provider --porcelain"
    if [[ -n "$filter_type" ]]; then
        cmd="$cmd --type=$filter_type"
    fi
    
    output=$(eval "$cmd" 2>/dev/null)
    
    if [[ -z "$output" ]]; then
        _forge_log error "No providers available"
        return 1
    fi
    
    # Filter by status if specified (e.g., "available" for configured providers)
    if [[ -n "$filter_status" ]]; then
        # Preserve the header line and filter the rest
        local header=$(echo "$output" | head -n 1)
        local filtered=$(echo "$output" | tail -n +2 | grep -i "$filter_status")
        if [[ -z "$filtered" ]]; then
            _forge_log error "No ${filter_status} providers found"
            return 1
        fi
        output=$(printf "%s\n%s" "$header" "$filtered")
    fi
    
    # Get current provider if not provided
    if [[ -z "$current_provider" ]]; then
        current_provider=$($_FORGE_BIN config get provider --porcelain 2>/dev/null)
    fi
    
    local fzf_args=(
        --delimiter="$_FORGE_DELIMITER"
        --prompt="Provider ❯ "
        --with-nth=1,3..
    )
    
    # Position cursor on current provider if available
    if [[ -n "$current_provider" ]]; then
        # For providers, compare against the first field (display name)
        local index=$(_forge_find_index "$output" "$current_provider" 1)
        fzf_args+=(--bind="start:pos($index)")
    fi
    
    local selected
    selected=$(echo "$output" | _forge_fzf --header-lines=1 "${fzf_args[@]}")
    
    if [[ -n "$selected" ]]; then
        echo "$selected"
        return 0
    fi
    
    return 1
}
