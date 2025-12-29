#!/usr/bin/env zsh

# Custom completion widget that handles both :commands and @ completion

# Helper function to show command completion with fzf
# Args:
#   $1 - optional filter text to pre-fill the query
# Returns: Sets BUFFER and CURSOR if a command is selected
function _forge_show_command_completion() {
    local filter_text="$1"
    
    # Lazily load the commands list
    local commands_list=$(_forge_get_commands)
    if [[ -z "$commands_list" ]]; then
        return 1
    fi
    
    # Use fzf for interactive selection with optional prefilled filter
    # TAB in fzf will select the highlighted command and insert it into the buffer
    local selected
    local fzf_bind="tab:accept"
    if [[ -n "$filter_text" ]]; then
        selected=$(echo "$commands_list" | _forge_fzf --header-lines=1 --delimiter="$_FORGE_DELIMITER" --nth=1 --query "$filter_text" --prompt="Command ❯ " --bind "$fzf_bind")
    else
        selected=$(echo "$commands_list" | _forge_fzf --header-lines=1 --delimiter="$_FORGE_DELIMITER" --nth=1 --prompt="Command ❯ " --bind "$fzf_bind")
    fi
    
    if [[ -n "$selected" ]]; then
        # Extract just the command name (first word before any description)
        local command_name="${selected%% *}"
        # Replace the current buffer with the selected command
        BUFFER=":$command_name "
        CURSOR=${#BUFFER}
        return 0
    fi
    
    return 1
}

function forge-completion() {
    local current_word="${LBUFFER##* }"
    
    # Handle @ completion (files and directories)
    if [[ "$current_word" =~ ^@.*$ ]]; then
        local filter_text="${current_word#@}"
        local selected
        local fzf_args=(
            --preview="if [ -d {} ]; then ls -la --color=always {} 2>/dev/null || ls -la {}; else $_FORGE_CAT_CMD {}; fi"
            $_FORGE_PREVIEW_WINDOW
        )
        
        local file_list=$($_FORGE_FD_CMD --type f --type d --hidden --exclude .git)
        if [[ -n "$filter_text" ]]; then
            selected=$(echo "$file_list" | _forge_fzf --query "$filter_text" "${fzf_args[@]}")
        else
            selected=$(echo "$file_list" | _forge_fzf "${fzf_args[@]}")
        fi
        
        if [[ -n "$selected" ]]; then
            selected="@[${selected}]"
            LBUFFER="${LBUFFER%$current_word}"
            BUFFER="${LBUFFER}${selected}${RBUFFER}"
            CURSOR=$((${#LBUFFER} + ${#selected}))
        fi
        
        zle reset-prompt
        return 0
    fi
    
    # Handle :command completion (supports letters, numbers, hyphens, underscores)
    if [[ "${LBUFFER}" =~ "^:([a-zA-Z][a-zA-Z0-9_-]*)?$" ]]; then
        # Extract the text after the colon for filtering
        local filter_text="${LBUFFER#:}"
        _forge_show_command_completion "$filter_text"
        zle reset-prompt
        return 0
    fi
    
    # Fall back to default completion
    zle expand-or-complete
}


# Auto-trigger widget for sentinel character ':'
# This widget is called when ':' is typed, and it automatically shows the fzf popup
# if the colon is at the start of the line
function forge-auto-complete() {
    # Insert the typed character first
    zle self-insert
    
    # Only trigger auto-completion if the buffer is exactly ":"
    # This means colon was typed at the start of the line
    if [[ "$BUFFER" == ":" ]]; then
        _forge_show_command_completion
        zle reset-prompt
    fi
}
