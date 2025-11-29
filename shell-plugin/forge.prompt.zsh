#!/usr/bin/env zsh

# Forge Prompt Customization Functions
# This file provides prompt helper functions for Forge AI assistant
# Documentation: PROMPT_CUSTOMIZATION.md

#################################################################################
# PUBLIC API: Prompt Customization Functions
#################################################################################
# These functions are exposed for manual integration into your prompts
#
# Environment Variables (direct access):
# - $_FORGE_ACTIVE_AGENT     : Current agent ID (e.g., "forge", "sage")
# - $_FORGE_CONVERSATION_ID  : Current conversation UUID (empty if no conversation)
# - $FORGE_PROMPT_ICON       : Icon displayed before agent name (default: 󰚩 U+F06A9)
#
# Usage Examples:
#
# 1. Simple ZSH integration:
#    PROMPT='$(prompt_forge_agent)%F{blue}%~%f %# '
#    RPROMPT='$(prompt_forge_model)'
#
# 2. Custom ZSH (using environment variables):
#    PROMPT='%B${(U)_FORGE_ACTIVE_AGENT}%b %F{blue}%~%f %# '
#
# 3. Powerlevel10k (add to your .p10k.zsh):
#    function prompt_forge_agent() {
#      local agent="${(U)_FORGE_ACTIVE_AGENT}"
#      [[ -n "$agent" ]] && p10k segment -t "$agent"
#    }
#    # Then add 'forge_agent' to POWERLEVEL9K_LEFT_PROMPT_ELEMENTS
#
# 4. Starship (add to ~/.config/starship.toml):
#    [custom.forge_agent]
#    command = "echo -n $_FORGE_ACTIVE_AGENT | tr '[:lower:]' '[:upper:]'"
#    when = '[ -n "$_FORGE_ACTIVE_AGENT" ]'
#    format = "[$output]($style) "
#    style = "bold white"
#
# 5. Show token count in your prompt:
#    RPROMPT='$(prompt_forge_model) [$(prompt_forge_message_count)]'

#################################################################################
# INTERNAL HELPERS
#################################################################################

# Returns the forge command to use (private helper)
function _prompt_forge_cmd() {
    echo "${_FORGE_BIN:-${FORGE_BIN:-forge}}"
}

#################################################################################
# PUBLIC API FUNCTIONS
#################################################################################

# Returns unstyled left prompt content (agent name with icon)
# Returns the agent name in uppercase with an icon prefix without any styling
#
# Example output: "󰚩 FORGE" or "" (empty if no agent)
#
# Example:
#   agent=$(prompt_forge_agent_unstyled)
#   PROMPT="%F{yellow}${agent} %f%~ %# "
function prompt_forge_agent_unstyled() {
    if [[ -n "$_FORGE_ACTIVE_AGENT" ]]; then
        if [[ -n "$FORGE_PROMPT_ICON" ]]; then
            echo "${FORGE_PROMPT_ICON} ${(U)_FORGE_ACTIVE_AGENT}"
        else
            echo "${(U)_FORGE_ACTIVE_AGENT}"
        fi
    fi
}

# Returns unstyled right prompt content (model name)
# Returns model without any styling
#
# Example output: "claude-3-5-sonnet" or "" (empty if no model)
#
# Example:
#   model=$(prompt_forge_model_unstyled)
#   RPROMPT="%F{blue}${model}%f"
function prompt_forge_model_unstyled() {
    local model_output=$($(_prompt_forge_cmd) config get model 2>/dev/null)
    
    if [[ -n "$model_output" ]]; then
        echo "${model_output}"
    fi
}

# Returns a styled left prompt segment (agent name)
# This is a ready-to-use function for ZSH prompts
#
# Format: BOLD UPPERCASE agent name
# Colors:
# - Bold dark grey when no conversation is active
# - Bold white when conversation is active
#
# Example:
#   PROMPT='$(prompt_forge_agent)%F{blue}%~%f %# '
function prompt_forge_agent() {
    local content=$(prompt_forge_agent_unstyled)
    if [[ -n "$content" ]]; then
        if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
            # Active: bold white
            echo "%B%F{white}${content}%f%b"
        else
            # Idle: bold dark grey
            echo "%B%F{8}${content}%f%b"
        fi
    fi
}

# Returns a styled right prompt segment (model name)
# This is a ready-to-use function for ZSH prompts
#
# Format: model name
# Color: Cyan (consistent, not context-dependent)
#
# Example:
#   RPROMPT='$(prompt_forge_model)'
function prompt_forge_model() {
    local content=$(prompt_forge_model_unstyled)
    if [[ -n "$content" ]]; then
        # Always cyan regardless of conversation state
        echo "%F{cyan}${content}%f"
    fi
}

# Returns unstyled token count for the current conversation
# Returns the token count in human-readable format (e.g., "10k", "1.2M")
#
# Example output: "42k" or "0" (when no conversation)
#
# Example:
#   count=$(prompt_forge_message_count_unstyled)
#   RPROMPT="%F{blue}Tokens: ${count}%f"
function prompt_forge_message_count_unstyled() {
    if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
        local stats_output=$($(_prompt_forge_cmd) conversation stats "$_FORGE_CONVERSATION_ID" --porcelain 2>/dev/null)
        
        if [[ -n "$stats_output" ]]; then
            # Extract total_tokens from porcelain output (format: "token  total_tokens      36000")
            local tokens=$(echo "$stats_output" | awk '/^token[[:space:]]+total_tokens/ {print $3}')
            
            if [[ -n "$tokens" ]]; then
                # Format tokens in human-readable format
                if (( tokens >= 1000000 )); then
                    # Format as millions (e.g., 1.2M, 0.7M)
                    printf "%.1fM" $(( tokens / 100000.0 / 10.0 ))
                elif (( tokens >= 1000 )); then
                    # Format as thousands (e.g., 10k, 100k)
                    printf "%dk" $(( tokens / 1000 ))
                else
                    # Less than 1000, show as-is
                    echo "$tokens"
                fi
                return
            fi
        fi
    fi
    
    # No conversation or no tokens - show 0
    echo "0"
}

# Returns the token count for the current conversation
# This is a ready-to-use function for ZSH prompts
#
# Format: token count in human-readable format (e.g., "10k", "1.2M")
#
# Colors:
# - Green when conversation is active
# - Dark grey when no conversation is active (shows "0")
#
# Example output: "42k" (in green) or "0" (in dark grey when no conversation)
#
# Example:
#   RPROMPT='$(prompt_forge_model) [$(prompt_forge_message_count)]'
function prompt_forge_message_count() {
    local content=$(prompt_forge_message_count_unstyled)
    if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
        # Active conversation: green
        echo "%F{green}${content}%f"
    else
        # No conversation: dark grey
        echo "%F{8}${content}%f"
    fi
}

# End of Public API
#################################################################################

#################################################################################
# POWERLEVEL9K/10K INTEGRATION HELPERS
#################################################################################
# These functions are ready-to-use with Powerlevel9k and Powerlevel10k
#
# To use, add these segment names to your prompt elements:
# - 'forge_agent' for the left prompt (agent name)
# - 'forge_model' for the right prompt (model name)
# - 'forge_message_count' for the right prompt (token count)
#
# Example in your .p10k.zsh or .zshrc:
#   POWERLEVEL9K_LEFT_PROMPT_ELEMENTS=(... forge_agent ...)
#   POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS=(... forge_model forge_message_count ...)
#
# Or for Powerlevel9k:
#   POWERLEVEL9K_LEFT_PROMPT_ELEMENTS=(context ... forge_agent dir vcs)
#   POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS=(status forge_model forge_message_count time)

# Powerlevel segment for agent name (left prompt)
# Applies consistent styling across P10k and P9k
#
# Usage: Add 'forge_agent' to POWERLEVEL9K_LEFT_PROMPT_ELEMENTS
function prompt_forge_agent_p9k() {
    local styled=$(prompt_forge_agent)
    if [[ -n "$styled" ]]; then
        # Remove trailing space for segments
        styled="${styled% }"
        
        # Check if p10k is available
        if (( $+functions[p10k] )); then
            # Powerlevel10k - use p10k segment with our styling
            p10k segment -t "$styled"
        else
            # Powerlevel9k - output directly
            echo -n "$styled"
        fi
    fi
}

# Powerlevel segment for model name (right prompt)
# Applies consistent styling across P10k and P9k
#
# Usage: Add 'forge_model' to POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS
function prompt_forge_model_p9k() {
    local styled=$(prompt_forge_model)
    if [[ -n "$styled" ]]; then
        # Check if p10k is available
        if (( $+functions[p10k] )); then
            # Powerlevel10k - use p10k segment with our styling
            p10k segment -t "$styled"
        else
            # Powerlevel9k - output directly
            echo -n "$styled"
        fi
    fi
}

# Powerlevel segment for token count (right prompt)
# Applies consistent styling across P10k and P9k
#
# Usage: Add 'forge_message_count' to POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS
function prompt_forge_message_count_p9k() {
    local styled=$(prompt_forge_message_count)
    if [[ -n "$styled" ]]; then
        # Check if p10k is available
        if (( $+functions[p10k] )); then
            # Powerlevel10k - use p10k segment with our styling
            p10k segment -t "$styled"
        else
            # Powerlevel9k - output directly
            echo -n "$styled"
        fi
    fi
}

# End of Powerlevel Integration
#################################################################################
