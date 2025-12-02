#!/usr/bin/env zsh

# Logging action handlers for log level management

# Action handler: Display or set log level
function _forge_action_log_level() {
    local new_level="$1"
    
    # Define valid levels
    local valid_levels=(debug info warning error success)
    
    if [[ -z "$new_level" ]]; then
        # Display current log level
        echo
        echo "Current log level: \033[1m${_FORGE_LOG_LEVEL}\033[0m"
        echo
        echo "Available levels:"
        for level in "${valid_levels[@]}"; do
            if [[ "$level" == "$_FORGE_LOG_LEVEL" ]]; then
                echo "  \033[32m✓\033[0m \033[1m${level}\033[0m (current)"
            else
                echo "    ${level}"
            fi
        done
        echo
        echo "Usage: :log-level <level>"
        echo "       :log-level"
        echo
        echo "Set FORGE_LOG_LEVEL environment variable to make permanent changes."
    else
        # Validate the new level
        local is_valid=0
        for level in "${valid_levels[@]}"; do
            if [[ "$level" == "$new_level" ]]; then
                is_valid=1
                break
            fi
        done
        
        if [[ $is_valid -eq 1 ]]; then
            # Set the new log level for current session
            typeset -h _FORGE_LOG_LEVEL="$new_level"
            _forge_log success "Log level set to \033[1m${new_level}\033[0m for current session"
            echo
            echo "To make this change permanent, set the environment variable:"
            echo "  export FORGE_LOG_LEVEL=${new_level}"
        else
            _forge_log error "Invalid log level: \033[1m${new_level}\033[0m"
            echo
            echo "Valid levels are: ${valid_levels[*]}"
            return 1
        fi
    fi
    
    _forge_reset
}