#!/usr/bin/env zsh
# Forge ZSH Theme
# A clean, modern theme designed for the Forge AI assistant
# Inspired by Spaceship and Starship themes

# Load version control information
autoload -Uz vcs_info
precmd_vcs_info() { vcs_info }
precmd_functions+=(precmd_vcs_info)
setopt prompt_subst

# Configure vcs_info for git with detailed stats
zstyle ':vcs_info:*' enable git
zstyle ':vcs_info:*' check-for-changes true
zstyle ':vcs_info:*' get-revision true
zstyle ':vcs_info:git:*' formats ' %F{green} %b%f'
zstyle ':vcs_info:git:*' actionformats ' %F{green} %b%f %F{red}(%a)%f'

# Icons
FORGE_FOLDER_ICON="${FORGE_FOLDER_ICON:-}"
FORGE_GIT_ICON="${FORGE_GIT_ICON:-}"
FORGE_MODEL_ICON="${FORGE_MODEL_ICON:-}"
FORGE_AGENT_ICON="${FORGE_AGENT_ICON:-󱙺}"
FORGE_PROMPT_SYMBOL="${FORGE_PROMPT_SYMBOL:-}"

# Get git stats (modified, staged, untracked)
function _git_stats() {
    if git rev-parse --git-dir > /dev/null 2>&1; then
        local modified=$(git diff --name-only 2>/dev/null | wc -l | tr -d ' ')
        local staged=$(git diff --cached --name-only 2>/dev/null | wc -l | tr -d ' ')
        local untracked=$(git ls-files --others --exclude-standard 2>/dev/null | wc -l | tr -d ' ')
        
        local stats=""
        if [[ $modified -gt 0 ]] || [[ $staged -gt 0 ]]; then
            stats="${modified} ${staged}!"
        fi
        if [[ $untracked -gt 0 ]]; then
            [[ -n $stats ]] && stats="${stats} "
            stats="${stats}${untracked}?"
        fi
        
        if [[ -n $stats ]]; then
            echo " %F{yellow}${stats}%f"
        fi
    fi
}

# Directory name with icon
function _forge_directory() {
    echo "%F{cyan}${FORGE_FOLDER_ICON} %1~%f"
}

# Git branch with icon and stats
function _forge_git() {
    if git rev-parse --git-dir > /dev/null 2>&1; then
        local branch=$(git symbolic-ref --short HEAD 2>/dev/null || git rev-parse --short HEAD 2>/dev/null)
        echo " %F{green}${FORGE_GIT_ICON} ${branch}%f$(_git_stats)"
    fi
}

# Model info with icon
# Color: dim (242) when no conversation, cyan when conversation active
function _forge_model() {
    local forge_bin="${_FORGE_BIN:-${FORGE_BIN:-forge}}"
    local model="${_FORGE_ACTIVE_MODEL}"
    
    if [[ -z "$model" ]]; then
        model=$($forge_bin config get model 2>/dev/null)
    fi
    
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
        local agent="${(U)_FORGE_ACTIVE_AGENT}"  # Convert to uppercase
        local count=""
        local color="242"  # Dim by default
        
        if [[ -n "$_FORGE_CONVERSATION_MESSAGE_COUNT" ]] && [[ "$_FORGE_CONVERSATION_MESSAGE_COUNT" != "0" ]]; then
            count=" ${_FORGE_CONVERSATION_MESSAGE_COUNT}"
            color="white"  # White when count exists
        fi
        
        echo "%B%F{${color}}${FORGE_AGENT_ICON} ${agent}${count}%f%b"
    fi
}

# Update forge variables on each prompt
function _update_forge_vars() {
    local forge_bin="${_FORGE_BIN:-${FORGE_BIN:-forge}}"
    
    # Update model if not set
    if [[ -z "$_FORGE_ACTIVE_MODEL" ]]; then
        _FORGE_ACTIVE_MODEL=$($forge_bin config get model 2>/dev/null)
    fi
    
    # Update token count if in a conversation
    if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
        local stats=$($forge_bin conversation stats "$_FORGE_CONVERSATION_ID" --porcelain 2>/dev/null)
        if [[ -n "$stats" ]]; then
            local tokens=$(echo "$stats" | awk '/^token[[:space:]]+total_tokens/ {print $3}')
            if [[ -n "$tokens" ]]; then
                if (( tokens >= 1000000 )); then
                    _FORGE_CONVERSATION_MESSAGE_COUNT=$(printf "%.1fM" $(( tokens / 100000.0 / 10.0 )))
                elif (( tokens >= 1000 )); then
                    _FORGE_CONVERSATION_MESSAGE_COUNT=$(printf "%dk" $(( tokens / 1000 )))
                else
                    _FORGE_CONVERSATION_MESSAGE_COUNT="$tokens"
                fi
            fi
        fi
    else
        _FORGE_CONVERSATION_MESSAGE_COUNT=""
    fi
}
precmd_functions+=(_update_forge_vars)

# Main prompt: directory + git + chevron
PROMPT='$(_forge_directory)$(_forge_git)%F{green}${FORGE_PROMPT_SYMBOL} %f '

# Right prompt: model + agent with token count
RPROMPT='$(_forge_agent) $(_forge_model)'

# Continuation prompt
PROMPT2='%F{242}...%f '

# Execution trace prompt  
PROMPT3='%F{242}?%f '

# Selection prompt for select command
PROMPT4='%F{242}+%f '
