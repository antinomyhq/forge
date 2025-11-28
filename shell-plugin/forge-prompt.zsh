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
#
# Usage Examples:
#
# 1. Simple ZSH integration:
#    PROMPT='$(forge_prompt_left)%F{blue}%~%f %# '
#    RPROMPT='$(forge_prompt_right)'
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

# Returns unstyled left prompt content (agent name)
# Returns just the agent name in uppercase without any styling
#
# Example output: "FORGE " or "" (empty if no agent)
#
# Example:
#   agent=$(forge_prompt_left_unstyled)
#   PROMPT="%F{yellow}${agent}%f%~ %# "
function forge_prompt_left_unstyled() {
    if [[ -n "$_FORGE_ACTIVE_AGENT" ]]; then
        echo "${(U)_FORGE_ACTIVE_AGENT} "
    fi
}

# Returns unstyled right prompt content (indicator + model name)
# Returns indicator and model without any styling
# - ○ (empty circle) when idle (no conversation)
# - ● (filled circle) when active (conversation in progress)
#
# Example output: "○ claude-3-5-sonnet" or "● claude-3-5-sonnet" or "" (empty if no model)
#
# Example:
#   model=$(forge_prompt_right_unstyled)
#   RPROMPT="%F{blue}${model}%f"
function forge_prompt_right_unstyled() {
    local forge_cmd="${_FORGE_BIN:-${FORGE_BIN:-forge}}"
    local model_output=$($forge_cmd config get model 2>/dev/null)
    
    if [[ -n "$model_output" ]]; then
        local indicator="○"
        if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
            indicator="●"
        fi
        echo "${indicator} ${model_output}"
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
#   PROMPT='$(forge_prompt_left)%F{blue}%~%f %# '
function forge_prompt_left() {
    local content=$(forge_prompt_left_unstyled)
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

# Returns a styled right prompt segment (model name with indicator)
# This is a ready-to-use function for ZSH prompts
#
# Format: indicator + model name
# - ○ (empty circle) when idle (no conversation)
# - ● (filled circle) when active (conversation in progress)
#
# Colors:
# - Dark grey when no conversation is active
# - Cyan when conversation is active
#
# Example:
#   RPROMPT='$(forge_prompt_right)'
function forge_prompt_right() {
    local content=$(forge_prompt_right_unstyled)
    if [[ -n "$content" ]]; then
        if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
            # Active: cyan
            echo "%F{cyan}${content}%f"
        else
            # Idle: dark grey
            echo "%F{8}${content}%f"
        fi
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
# - 'forge_model' for the right prompt (model with indicator)
#
# Example in your .p10k.zsh or .zshrc:
#   POWERLEVEL9K_LEFT_PROMPT_ELEMENTS=(... forge_agent ...)
#   POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS=(... forge_model ...)
#
# Or for Powerlevel9k:
#   POWERLEVEL9K_LEFT_PROMPT_ELEMENTS=(context ... forge_agent dir vcs)
#   POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS=(status forge_model time)

# Powerlevel segment for agent name (left prompt)
# Applies consistent styling across P10k and P9k
#
# Usage: Add 'forge_agent' to POWERLEVEL9K_LEFT_PROMPT_ELEMENTS
function prompt_forge_agent_p9k() {
    local content=$(forge_prompt_left_unstyled)
    if [[ -n "$content" ]]; then
        # Remove trailing space for segments
        content="${content% }"
        
        # Apply consistent styling for both P10k and P9k
        if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
            # Active: bold white
            local styled="%B%F{white}${content}%f%b"
        else
            # Idle: bold dark grey
            local styled="%B%F{8}${content}%f%b"
        fi
        
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

# Powerlevel segment for model name with indicator (right prompt)
# Applies consistent styling across P10k and P9k
#
# Usage: Add 'forge_model' to POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS
function prompt_forge_model_p9k() {
    local content=$(forge_prompt_right_unstyled)
    if [[ -n "$content" ]]; then
        # Apply consistent styling for both P10k and P9k
        if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
            # Active: cyan
            local styled="%F{cyan}${content}%f"
        else
            # Idle: dark grey
            local styled="%F{8}${content}%f"
        fi
        
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
