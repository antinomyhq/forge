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

# Async git stats with caching for maximum performance
# Cache variables
typeset -g _FORGE_GIT_STATS_CACHE=""
typeset -g _FORGE_GIT_STATS_PWD=""
typeset -g _FORGE_GIT_STATS_PID=0

# Async worker function (runs in background)
function _git_stats_async_worker() {
    local status_output=$(git status --porcelain 2>/dev/null)
    
    # Early exit if no changes
    [[ -z "$status_output" ]] && return
    
    local modified=0 staged=0 untracked=0
    
    # Parse status in a single pass (optimized with awk)
    echo "$status_output" | awk '
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
    }'
}

# Get git stats (cached and async)
function _git_stats() {
    # Only run if we're in a git repo (vcs_info already checked this)
    [[ -z "$vcs_info_msg_0_" ]] && return
    
    local current_pwd="$PWD"
    
    # Return cached result if in same directory and worker is not running
    if [[ "$_FORGE_GIT_STATS_PWD" == "$current_pwd" ]] && [[ $_FORGE_GIT_STATS_PID -eq 0 ]]; then
        [[ -n "$_FORGE_GIT_STATS_CACHE" ]] && echo " %F{yellow}${_FORGE_GIT_STATS_CACHE}%f"
        return
    fi
    
    # If directory changed or no cache, start async worker
    if [[ "$_FORGE_GIT_STATS_PWD" != "$current_pwd" ]] || [[ -z "$_FORGE_GIT_STATS_CACHE" ]]; then
        # Kill previous worker if still running
        if [[ $_FORGE_GIT_STATS_PID -gt 0 ]]; then
            kill -0 $_FORGE_GIT_STATS_PID 2>/dev/null && kill $_FORGE_GIT_STATS_PID 2>/dev/null
        fi
        
        # Start new async worker
        {
            local result=$(_git_stats_async_worker)
            # Update cache atomically using a temp file
            local tmpfile="${TMPDIR:-/tmp}/forge_git_stats_$$"
            echo "$result" > "$tmpfile"
            echo "$current_pwd" >> "$tmpfile"
            # Signal completion by writing PID 0
            echo "0" >> "$tmpfile"
        } &!
        
        # Store worker PID
        _FORGE_GIT_STATS_PID=$!
        
        # Update directory tracker
        _FORGE_GIT_STATS_PWD="$current_pwd"
        
        # Show cached result from previous directory (better than nothing)
        [[ -n "$_FORGE_GIT_STATS_CACHE" ]] && echo " %F{yellow}${_FORGE_GIT_STATS_CACHE}%f"
        return
    fi
    
    # Show current cache
    [[ -n "$_FORGE_GIT_STATS_CACHE" ]] && echo " %F{yellow}${_FORGE_GIT_STATS_CACHE}%f"
}

# Update git stats cache from async worker
function _update_git_stats_cache() {
    local tmpfile="${TMPDIR:-/tmp}/forge_git_stats_$$"
    
    # Check if async worker completed
    if [[ -f "$tmpfile" ]]; then
        # Read results
        local result=$(sed -n '1p' "$tmpfile" 2>/dev/null)
        local pwd=$(sed -n '2p' "$tmpfile" 2>/dev/null)
        
        # Update cache if pwd matches
        if [[ "$pwd" == "$PWD" ]]; then
            _FORGE_GIT_STATS_CACHE="$result"
        fi
        
        # Clear worker PID
        _FORGE_GIT_STATS_PID=0
        
        # Clean up temp file
        rm -f "$tmpfile"
        
        # Trigger prompt refresh to show new results
        zle && zle reset-prompt
    fi
}

# Check for async results before each prompt
precmd_functions+=(_update_git_stats_cache)

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
PROMPT='$(_forge_directory)$(_forge_git) %F{green}${FORGE_PROMPT_SYMBOL}%f '

# Right prompt: model + agent with token count
RPROMPT='$(_forge_agent) $(_forge_model)'

# Continuation prompt
PROMPT2='%F{242}...%f '

# Execution trace prompt  
PROMPT3='%F{242}?%f '

# Selection prompt for select command
PROMPT4='%F{242}+%f '
