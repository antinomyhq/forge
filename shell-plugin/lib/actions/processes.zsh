#!/usr/bin/env zsh

# Action handlers for background process management

# Action handler: List and manage background processes
# Uses fzf to select a process, then offers to kill it and optionally delete
# the log file.
function _forge_action_processes() {
    echo
    
    # Get process list in porcelain format (tab-separated)
    local output=$($_FORGE_BIN processes --porcelain 2>/dev/null)
    
    if [[ -z "$output" ]]; then
        _forge_log info "No background processes running"
        return 0
    fi
    
    # Build display lines for fzf
    local -a display_lines
    local -a pids
    local -a cwds
    while IFS=$'\t' read -r pid proc_status command cwd started_at log_file; do
        display_lines+=("PID ${pid} | ${proc_status} | ${command} | ${cwd} | log: ${log_file}")
        pids+=("$pid")
        cwds+=("$cwd")
    done <<< "$output"
    
    # Use fzf to select a process
    local selected=$(printf '%s\n' "${display_lines[@]}" | fzf --prompt="Select process to kill > " --height=~40% --border)
    
    if [[ -z "$selected" ]]; then
        return 0
    fi
    
    # Extract PID from selection
    local selected_pid=$(echo "$selected" | grep -oP 'PID \K[0-9]+')
    
    if [[ -z "$selected_pid" ]]; then
        _forge_log error "Failed to parse PID from selection"
        return 1
    fi
    
    # Kill the process (keep log file by default)
    $_FORGE_BIN processes --kill "$selected_pid" 2>/dev/null
    _forge_log info "Killed process ${selected_pid}"
    
    # Ask about log file deletion
    echo -n "Delete the log file? [y/N] "
    read -r response
    if [[ "${response:l}" == "y" || "${response:l}" == "yes" ]]; then
        $_FORGE_BIN processes --kill "$selected_pid" --delete-log 2>/dev/null
        _forge_log info "Deleted log file"
    fi
}
