#!/usr/bin/env zsh
# Forge ZSH Theme
# A clean, modern theme designed for the Forge AI assistant
# Inspired by Spaceship and Starship themes

# Load version control information
autoload -Uz vcs_info
precmd_vcs_info() { vcs_info }
precmd_functions+=(precmd_vcs_info)
setopt prompt_subst

# Configure vcs_info for git (optimized and cached by zsh)
zstyle ':vcs_info:*' enable git
zstyle ':vcs_info:git:*' formats '%b'
zstyle ':vcs_info:git:*' actionformats '%b %F{red}(%a)%f'

# Icons
FORGE_FOLDER_ICON="${FORGE_FOLDER_ICON:-}"
FORGE_GIT_ICON="${FORGE_GIT_ICON:-}"
FORGE_MODEL_ICON="${FORGE_MODEL_ICON:-}"
FORGE_AGENT_ICON="${FORGE_AGENT_ICON:-󱙺}"
FORGE_PROMPT_SYMBOL="${FORGE_PROMPT_SYMBOL:-}"

# Get git stats (synchronous)
function _git_stats() {
    # Only run if we're in a git repo (vcs_info already checked this)
    [[ -z "$vcs_info_msg_0_" ]] && return
    
    local status_output=$(git status --porcelain 2>/dev/null)
    
    # Early exit if no changes
    [[ -z "$status_output" ]] && return
    
    # Parse status in a single pass (optimized with awk)
    local result=$(echo "$status_output" | awk '
    BEGIN { m=0; s=0; u=0 }
    /^\?\?/ { u++ }
    /^ [MD]/ { m++ }
    /^[MADRC] / { s++ }
    /^[MAD][MD]/ { m++; s++ }
    END { 
        if (m > 0 || s > 0) printf "%d %d!", m, s
        if (u > 0) {
            if (m > 0 || s > 0) printf " "
            printf "%d?", u
        }
    }')
    
    [[ -n "$result" ]] && echo " %F{yellow}${result}%f"
}

# Directory name with icon
function _forge_directory() {
    echo "%F{cyan}${FORGE_FOLDER_ICON} %1~%f"
}

# Git branch with icon and stats
# Uses vcs_info (cached by zsh) for branch name + single git call for stats
function _forge_git() {
    if [[ -n "$vcs_info_msg_0_" ]]; then
        echo " %F{green}${FORGE_GIT_ICON} ${vcs_info_msg_0_}%f$(_git_stats)"
    fi
}

# Model info with icon
# Color: dim (242) when no conversation, cyan when conversation active
function _forge_model() {
    local forge_bin="${_FORGE_BIN:-${FORGE_BIN:-forge}}"
    local model=$($forge_bin config get model 2>/dev/null)
    
    if [[ -n "$model" ]]; then
        local color="242"  # Dim by default
        if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
            color="cyan"  # Cyan when conversation active
        fi
        echo "%F{${color}}${FORGE_MODEL_ICON} ${model}%f"
    fi
}

# Agent with token count
# Color: dim (242) when no count, white when count > 0
function _forge_agent() {
    if [[ -n "$_FORGE_ACTIVE_AGENT" ]]; then
        local forge_bin="${_FORGE_BIN:-${FORGE_BIN:-forge}}"
        local agent="${(U)_FORGE_ACTIVE_AGENT}"  # Convert to uppercase
        local count=""
        local color="242"  # Dim by default
        
        # Get token count from forge command if in a conversation
        if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
            local stats=$($forge_bin conversation stats "$_FORGE_CONVERSATION_ID" --porcelain 2>/dev/null)
            if [[ -n "$stats" ]]; then
                local tokens=$(echo "$stats" | awk '/^token[[:space:]]+total_tokens/ {print $3}')
                if [[ -n "$tokens" ]] && [[ "$tokens" != "0" ]]; then
                    # Format tokens in human-readable format
                    if (( tokens >= 1000000 )); then
                        count=$(printf " %.1fM" $(( tokens / 100000.0 / 10.0 )))
                    elif (( tokens >= 1000 )); then
                        count=$(printf " %dk" $(( tokens / 1000 )))
                    else
                        count=" $tokens"
                    fi
                    color="white"  # White when count exists
                fi
            fi
        fi
        
        echo "%B%F{${color}}${FORGE_AGENT_ICON} ${agent}${count}%f%b"
    fi
}

# Main prompt: directory + git + chevron
PROMPT='$(_forge_directory)$(_forge_git) %F{green}${FORGE_PROMPT_SYMBOL}%f '

# Right prompt: model + agent with token count
RPROMPT='$(_forge_agent) $(_forge_model)'

# Continuation prompt
PROMPT2='%F{242}...%f '

# Execution trace prompt  
PROMPT3='%F{242}?%f '

# Selection prompt for select command
PROMPT4='%F{242}+%f '
