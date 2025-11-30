#!/usr/bin/env zsh

# Core utility functions for forge plugin

# Lazy loader for commands cache
# Loads the commands list only when first needed, avoiding startup cost
function _forge_get_commands() {
    if [[ -z "$_FORGE_COMMANDS" ]]; then
        _FORGE_COMMANDS="$(CLICOLOR_FORCE=0 $_FORGE_BIN list commands --porcelain 2>/dev/null)"
    fi
    echo "$_FORGE_COMMANDS"
}

# Private fzf function with common options for consistent UX
function _forge_fzf() {
    fzf --exact --cycle --select-1 --height 100% --no-scrollbar "$@"
}

# Helper function to execute forge commands consistently
# This ensures proper handling of special characters and consistent output
function _forge_exec() {
    # Ensure FORGE_ACTIVE_AGENT always has a value, default to "forge"
    local agent_id="${_FORGE_ACTIVE_AGENT:-forge}"
    
    eval "$_FORGE_BIN --agent $(printf '%q' "$agent_id") $(printf '%q ' "$@")"
}

# Helper function to clear buffer and reset prompt
function _forge_reset() {
    # Invoke precmd hooks to ensure prompt customizations (starship, oh-my-zsh themes, etc.) refresh properly
    for precmd in $precmd_functions; do
        if typeset -f "$precmd" >/dev/null 2>&1; then
            "$precmd"
        fi
    done

   BUFFER=""
   CURSOR=0

   zle reset-prompt 
    
}

# Helper function to find the index of a value in a list (1-based)
# Returns the index if found, 1 otherwise
# Usage: _forge_find_index <output> <value_to_find> [field_number]
# field_number: which field to compare (1 for first field, 2 for second field, etc.)
function _forge_find_index() {
    local output="$1"
    local value_to_find="$2"
    local field_number="${3:-1}"  # Default to first field if not specified

    local index=1
    while IFS= read -r line; do
        # Extract the specified field for comparison
        local field_value=$(echo "$line" | awk "{print \$$field_number}")
        if [[ "$field_value" == "$value_to_find" ]]; then
            echo "$index"
            return 0
        fi
        ((index++))
    done <<< "$output"

    echo "1"
    return 0
}

# Helper function to print messages with consistent formatting based on log level
# Usage: _forge_log <level> <message>
# Levels: error, info, success, warning, debug
# Color scheme matches crates/forge_main/src/title_display.rs
function _forge_log() {
    local level="$1"
    local message="$2"
    local timestamp="\033[90m[$(date '+%H:%M:%S')]\033[0m"
    
    case "$level" in
        error)
            # Category::Error - Red ❌
            echo "\033[31m❌\033[0m ${timestamp} \033[31m${message}\033[0m"
            ;;
        info)
            # Category::Info - White ⏺
            echo "\033[37m⏺\033[0m ${timestamp} \033[37m${message}\033[0m"
            ;;
        success)
            # Category::Action/Completion - Yellow ⏺
            echo "\033[33m⏺\033[0m ${timestamp} \033[37m${message}\033[0m"
            ;;
        warning)
            # Category::Warning - Bright yellow ⚠️
            echo "\033[93m⚠️\033[0m ${timestamp} \033[93m${message}\033[0m"
            ;;
        debug)
            # Category::Debug - Cyan ⏺ with dimmed text
            echo "\033[36m⏺\033[0m ${timestamp} \033[90m${message}\033[0m"
            ;;
        *)
            echo "${message}"
            ;;
    esac
}

# Helper function to select a provider from the list
# Usage: _forge_select_provider [filter_status] [current_provider]
# Returns: selected provider line (via stdout)
function _forge_select_provider() {
    local filter_status="${1:-}"
    local current_provider="${2:-}"
    local output
    output=$($_FORGE_BIN list provider --porcelain 2>/dev/null)
    
    if [[ -z "$output" ]]; then
        _forge_log error "No providers available"
        return 1
    fi
    
    # Filter by status if specified (e.g., "available" for configured providers)
    if [[ -n "$filter_status" ]]; then
        output=$(echo "$output" | grep -i "$filter_status")
        if [[ -z "$output" ]]; then
            _forge_log error "No ${filter_status} providers found"
            return 1
        fi
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
    selected=$(echo "$output" | _forge_fzf "${fzf_args[@]}")
    
    if [[ -n "$selected" ]]; then
        echo "$selected"
        return 0
    fi
    
    return 1
}
