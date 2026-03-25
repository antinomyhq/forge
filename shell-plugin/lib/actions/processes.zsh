#!/usr/bin/env zsh

# Action handlers for background process management

# Action handler: List and manage background processes
# Uses fzf with log file preview to select a process, then offers to kill it.
# Porcelain format: PID  COMMAND  NAME  LOG (multi-space aligned with header)
# PID is hidden from fzf display using --with-nth=2..
function _forge_action_processes() {
    echo

    local output=$($_FORGE_BIN processes --porcelain 2>/dev/null)

    if [[ -z "$output" ]]; then
        _forge_log info "No background processes running"
        return 0
    fi

    # fzf shows columns 2+ (COMMAND, NAME, LOG), hiding PID (col 1).
    # {1} = PID, {-1} = LOG path for preview.
    local selected
    selected=$(echo "$output" | \
        _forge_fzf \
            --header-lines=1 \
            --delimiter="$_FORGE_DELIMITER" \
            --with-nth=2.. \
            --prompt="Kill a process ❯ " \
            --preview="${_FORGE_CAT_CMD} {-1}" \
            $_FORGE_PREVIEW_WINDOW)

    if [[ -z "$selected" ]]; then
        return 0
    fi

    # Extract PID (first multi-space delimited field, hidden from display)
    local selected_pid="${selected%%  *}"
    selected_pid="${selected_pid## }"
    selected_pid="${selected_pid%% }"

    if [[ -z "$selected_pid" ]]; then
        _forge_log error "Failed to parse PID from selection"
        return 1
    fi

    # Extract log file path (last field)
    local selected_log="${selected##*  }"
    selected_log="${selected_log## }"
    selected_log="${selected_log%% }"

    # Kill the process
    $_FORGE_BIN processes --kill "$selected_pid" 2>/dev/null

    # Ask about log file deletion
    if [[ -n "$selected_log" && -f "$selected_log" ]]; then
        echo -n "Delete the log file? [y/N] "
        read -r response
        if [[ "${response:l}" == "y" || "${response:l}" == "yes" ]]; then
            rm -f "$selected_log" 2>/dev/null
            _forge_log info "Deleted log file: ${selected_log}"
        fi
    fi
}
