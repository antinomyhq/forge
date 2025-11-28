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

# Returns unstyled left prompt content (agent name)
# Returns just the agent name in uppercase without any styling
#
# Example output: "FORGE" or "" (empty if no agent)
#
# Example:
#   agent=$(prompt_forge_agent_unstyled)
#   PROMPT="%F{yellow}${agent} %f%~ %# "
function prompt_forge_agent_unstyled() {
    if [[ -n "$_FORGE_ACTIVE_AGENT" ]]; then
        echo "${(U)_FORGE_ACTIVE_AGENT}"
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
#   model=$(prompt_forge_model_unstyled)
#   RPROMPT="%F{blue}${model}%f"
function prompt_forge_model_unstyled() {
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
#   RPROMPT='$(prompt_forge_model)'
function prompt_forge_model() {
    local content=$(prompt_forge_model_unstyled)
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

# Powerlevel segment for model name with indicator (right prompt)
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

# End of Powerlevel Integration
#################################################################################
