#!/usr/bin/env zsh

# Plugin management action handlers

# Action handler: Manage plugins
# Subcommands: list, enable <name>, disable <name>, info <name>, reload, install <path>
function _forge_action_plugin() {
    local input_text="$1"
    
    echo
    
    if [[ -z "$input_text" ]]; then
        # Default to list
        _forge_exec plugin list
        return 0
    fi
    
    # Parse subcommand and arguments
    local subcmd="${input_text%% *}"
    local args="${input_text#* }"
    
    # If no space was found, args equals subcmd (no arguments)
    if [[ "$args" == "$subcmd" ]]; then
        args=""
    fi
    
    case "$subcmd" in
        list|ls)
            _forge_exec plugin list
        ;;
        enable)
            if [[ -z "$args" ]]; then
                _forge_log error "Usage: :plugin enable <name>"
                return 0
            fi
            _forge_exec plugin enable "$args"
        ;;
        disable)
            if [[ -z "$args" ]]; then
                _forge_log error "Usage: :plugin disable <name>"
                return 0
            fi
            _forge_exec plugin disable "$args"
        ;;
        info)
            if [[ -z "$args" ]]; then
                _forge_log error "Usage: :plugin info <name>"
                return 0
            fi
            _forge_exec plugin info "$args"
        ;;
        reload)
            _forge_exec plugin reload
        ;;
        install)
            if [[ -z "$args" ]]; then
                _forge_log error "Usage: :plugin install <path>"
                return 0
            fi
            _forge_exec_interactive plugin install "$args"
        ;;
        *)
            _forge_log error "Unknown plugin subcommand '${subcmd}'. Expected: list, enable, disable, info, reload, install"
        ;;
    esac
}
