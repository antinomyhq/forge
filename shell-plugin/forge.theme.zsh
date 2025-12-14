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

# Model and agent info with token count
# Parallel execution of forge commands for better performance
# Uses porcelain format for reliable parsing
function _forge_prompt_info() {
    local forge_bin="${_FORGE_BIN:-${FORGE_BIN:-forge}}"
    local agent="${_FORGE_ACTIVE_AGENT}"
    local cid="${_FORGE_CONVERSATION_ID}"
    
    local model=""
    local tokens=""
    
    if [[ -n "$cid" ]]; then
        # Both commands needed - run in parallel
        # Use command substitution in background jobs
        local model_result
        local tokens_result
        
        # Start model fetch in background
        {
            model_result=$($forge_bin config get model 2>/dev/null)
            print -r -- "$model_result"
        } > >(read -r model) &
        local model_pid=$!
        
        # Start stats fetch in background
        {
            local stats=$($forge_bin conversation stats "$cid" --porcelain 2>/dev/null)
            if [[ -n "$stats" ]]; then
                tokens_result=$(echo "$stats" | awk '/^token[[:space:]]+total_tokens/ {print $3}')
            fi
            print -r -- "$tokens_result"
        } > >(read -r tokens) &
        local stats_pid=$!
        
        # Wait for both to complete
        wait $model_pid $stats_pid 2>/dev/null
    else
        # Only model needed - single call
        model=$($forge_bin config get model 2>/dev/null)
    fi
    
    # Build model display
    local model_display=""
    if [[ -n "$model" ]]; then
        local model_color="242"  # Dim by default
        if [[ -n "$cid" ]]; then
            model_color="cyan"  # Cyan when conversation active
        fi
        model_display="%F{${model_color}}${FORGE_MODEL_ICON} ${model}%f"
    fi
    
    # Build agent display with token count
    local agent_display=""
    if [[ -n "$agent" ]]; then
        local agent_upper="${(U)agent}"  # Convert to uppercase
        local count=""
        local agent_color="242"  # Dim by default
        
        # Format token count if available and non-zero
        if [[ -n "$tokens" ]] && [[ "$tokens" != "0" ]]; then
            # Format tokens in human-readable format
            if (( tokens >= 1000000 )); then
                count=$(printf " %.1fM" $(( tokens / 100000.0 / 10.0 )))
            elif (( tokens >= 1000 )); then
                count=$(printf " %dk" $(( tokens / 1000 )))
            else
                count=" $tokens"
            fi
            agent_color="white"  # White when count exists
        fi
        
        agent_display="%B%F{${agent_color}}${FORGE_AGENT_ICON} ${agent_upper}${count}%f%b"
    fi
    
    # Return both displays
    echo "${agent_display} ${model_display}"
}

# Main prompt: directory + git + chevron
PROMPT='$(_forge_directory)$(_forge_git) %F{green}${FORGE_PROMPT_SYMBOL}%f '

# Right prompt: agent and model with token count (single forge call)
RPROMPT='$(_forge_prompt_info)'

# Continuation prompt
PROMPT2='%F{242}...%f '

# Execution trace prompt  
PROMPT3='%F{242}?%f '

# Selection prompt for select command
PROMPT4='%F{242}+%f '
