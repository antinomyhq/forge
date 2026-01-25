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
    fzf --exact --cycle --select-1 --height 100% --no-scrollbar --ansi --color="header:bold" "$@"
}

# Helper function to execute forge commands consistently
# This ensures proper handling of special characters and consistent output
function _forge_exec() {
    # Ensure FORGE_ACTIVE_AGENT always has a value, default to "forge"
    local agent_id="${_FORGE_ACTIVE_AGENT:-forge}"
    
    eval "$_FORGE_BIN --agent $(printf '%q' "$agent_id") $(printf '%q ' "$@")"
}

function _forge_reset() {
  zle -I
  BUFFER=""
  CURSOR=0
  zle -R
  zle reset-prompt
}


# Helper function to find the index of a value in a list (1-based)
# Returns the index if found, 1 otherwise
# Usage: _forge_find_index <output> <value_to_find> [field_number]
# field_number: which field to compare (1 for first field, 2 for second field, etc.)
# Note: This function expects porcelain output WITH headers and skips the header line
function _forge_find_index() {
    local output="$1"
    local value_to_find="$2"
    local field_number="${3:-1}"  # Default to first field if not specified

    local index=1
    local line_num=0
    while IFS= read -r line; do
        ((line_num++))
        # Skip the header line (first line)
        if [[ $line_num -eq 1 ]]; then
            continue
        fi
        
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
            # Category::Error - Red ⏺
            echo "\033[31m⏺\033[0m ${timestamp} \033[31m${message}\033[0m"
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

# Helper function to check if a workspace is indexed
# Usage: _forge_is_workspace_indexed <workspace_path>
# Returns: 0 if workspace is indexed, 1 otherwise
function _forge_is_workspace_indexed() {
    local workspace_path="$1"
    $_FORGE_BIN workspace info "$workspace_path" >/dev/null 2>&1
    return $?
}

# Start background sync job for current workspace if not already running
# Uses canonical path hash to identify workspace
function _forge_start_background_sync() {
    # Check if sync is enabled (default to true if not set)
    local sync_enabled="${FORGE_SYNC_ENABLED:-true}"
    if [[ "$sync_enabled" != "true" ]]; then
        return 0
    fi
    
    # Get canonical workspace path
    local workspace_path=$(pwd -P)
    
    # Check if workspace is indexed before attempting sync
    if ! _forge_is_workspace_indexed "$workspace_path"; then
        return 0
    fi
    
    # Run sync once in background
    # Close all output streams immediately to prevent any flashing
    {
        exec >/dev/null 2>&1
        setopt NO_NOTIFY NO_MONITOR
        $_FORGE_BIN workspace sync "$workspace_path"
    } &!
}

