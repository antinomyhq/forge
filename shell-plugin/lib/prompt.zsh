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
# - $_FORGE_PROVIDER         : Current provider name (e.g., "openai", "anthropic")
# - $_FORGE_ACTIVE_MODEL     : Current model name (e.g., "claude-3-5-sonnet")
# - $_FORGE_MESSAGE_COUNT    : Message count for current conversation (human-readable, e.g., "42k", "1.2M")
#
# Usage Examples:
#
# 1. Simple ZSH integration:
#    PROMPT='$(prompt_forge_agent)%F{blue}%~%f %# '
#    RPROMPT='$(prompt_forge_model)'
#
# 2. Custom ZSH (using environment variables):
#    PROMPT='%B${_FORGE_ACTIVE_AGENT:u}%b %F{blue}%~%f %# '
#    RPROMPT='%F{cyan}${_FORGE_ACTIVE_MODEL}%f %F{green}${_FORGE_MESSAGE_COUNT}%f'
#
# 3. Powerlevel10k (add to your .p10k.zsh):
#    function prompt_forge_agent() {
#      local agent="${_FORGE_ACTIVE_AGENT:u}"
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
#
# 6. Powerlevel10k integration:

#################################################################################
# INTERNAL HELPERS
#################################################################################

# Returns the forge command to use (private helper)
function _prompt_forge_cmd() {
    echo "${_FORGE_BIN:-${FORGE_BIN:-forge}}"
}


# Internal helper for Powerlevel9k/10k segment rendering
# Takes styled content and renders it appropriately for P10k or P9k
#
# Args:
#   $1 - styled content to display
function _prompt_forge_p9k_segment() {
    local styled="$1"
    # Strip leading and trailing whitespace
    styled="${styled#"${styled%%[![:space:]]*}"}"
    styled="${styled%"${styled##*[![:space:]]}"}"
    
    if [[ -n "$styled" ]]; then
        # Check if p10k is available
        if typeset -f p10k >/dev/null 2>&1; then
            # Powerlevel10k - use p10k segment with our styling
            p10k segment -t "$styled"
        else
            # Powerlevel9k - output directly
            echo -n "$styled"
        fi
    fi
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
            echo "${FORGE_PROMPT_ICON} ${_FORGE_ACTIVE_AGENT:u}"
        else
            echo "${_FORGE_ACTIVE_AGENT:u}"
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

# Returns unstyled right prompt content (provider name)
# Returns provider without any styling
#
# Example output: "openai" or "" (empty if no provider)
#
# Example:
#   provider=$(prompt_forge_provider_unstyled)
#   RPROMPT="%F{blue}${provider}%f"
function prompt_forge_provider_unstyled() {
    local provider_output=$($(_prompt_forge_cmd) config get provider --porcelain 2>/dev/null)
    
    if [[ -n "$provider_output" ]]; then
        echo "${provider_output}"
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

# Returns a styled right prompt segment (provider name)
# This is a ready-to-use function for ZSH prompts
#
# Format: provider name
# Color: Cyan (consistent with model)
#
# Example:
#   RPROMPT='$(prompt_forge_provider)'
function prompt_forge_provider() {
    local content=$(prompt_forge_provider_unstyled)
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
                    local million_value=$(( tokens / 100000 ))
                    local decimal_value=$(( million_value % 10 ))
                    printf "%d.%dM" $(( million_value / 10 )) $decimal_value
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
    # Active conversation: green
    echo "%F{green}${content}%f"    
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
# - 'forge_provider' for the right prompt (provider name)
# - 'forge_message_count' for the right prompt (token count)
#
# Example in your .p10k.zsh or .zshrc:
#   POWERLEVEL9K_LEFT_PROMPT_ELEMENTS=(... forge_agent ...)
#   POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS=(... forge_model forge_provider forge_message_count ...)
#
# Or for Powerlevel9k:
#   POWERLEVEL9K_LEFT_PROMPT_ELEMENTS=(context ... forge_agent dir vcs)
#   POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS=(status forge_model forge_provider forge_message_count time)

# Powerlevel segment for agent name (left prompt)
# Applies consistent styling across P10k and P9k
#
# Usage: Add 'forge_agent' to POWERLEVEL9K_LEFT_PROMPT_ELEMENTS
function prompt_forge_agent_p9k() {
    _prompt_forge_p9k_segment "$(prompt_forge_agent)"
}

# Powerlevel segment for model name (right prompt)
# Applies consistent styling across P10k and P9k
#
# Usage: Add 'forge_model' to POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS
function prompt_forge_model_p9k() {
    _prompt_forge_p9k_segment "$(prompt_forge_model)"
}

# Powerlevel segment for provider name (right prompt)
# Applies consistent styling across P10k and P9k
#
# Usage: Add 'forge_provider' to POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS
function prompt_forge_provider_p9k() {
    _prompt_forge_p9k_segment "$(prompt_forge_provider)"
}

# Powerlevel segment for token count (right prompt)
# Applies consistent styling across P10k and P9k
#
# Usage: Add 'forge_message_count' to POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS
function prompt_forge_message_count_p9k() {
    _prompt_forge_p9k_segment "$(prompt_forge_message_count)"
}

# End of Powerlevel Integration
#################################################################################


# Update terminal title based on current forge state
function _forge_update_terminal_title() {
    # Skip if not in interactive terminal or if TERM doesn't support titles
    [[ ! -t 1 || "$TERM" == "dumb" || "$TERM" == "linux" ]] && return
    
    local title_parts=()
    
    # Add agent name if set
    if [[ -n "$_FORGE_ACTIVE_AGENT" ]]; then
        title_parts+=("${_FORGE_ACTIVE_AGENT:u}")
    fi
    
    # Add conversation info if active
    if [[ -n "$_FORGE_CONVERSATION_ID" ]]; then
        local conv_title=$(_forge_get_conversation_title "$_FORGE_CONVERSATION_ID")
        if [[ -n "$conv_title" && "$conv_title" != "<untitled>" ]]; then
            title_parts+=("$conv_title")
        else
            title_parts+=("$_FORGE_CONVERSATION_ID")
        fi
    fi
    
    # Build and set title
    if [[ ${#title_parts[@]} -gt 0 ]]; then
        local title="${title_parts[1]}"
        local i=2
        while [[ $i -le ${#title_parts[@]} ]]; do
            title="$title - ${title_parts[$i]}"
            ((i++))
        done
        printf '\033]0;%s\007' "$title"
    fi
}

# Get conversation title by ID
function _forge_get_conversation_title() {
    local conv_id="$1"
    [[ -z "$conv_id" ]] && return
    
    local conv_info
    conv_info=$($_FORGE_BIN conversation info "$conv_id" 2>/dev/null)
    if [[ -n "$conv_info" ]]; then
        # Remove ANSI codes and extract title
        local clean_info=$(echo "$conv_info" | sed 's/\x1b\[[0-9;]*m//g')
        local title=$(echo "$clean_info" | grep '^  title ' | sed 's/^  title //')
        [[ -n "$title" ]] && echo "$title"
    fi
}

update_forge_variables() {
    export _FORGE_ACTIVE_MODEL=$($_FORGE_BIN config get model)
    export _FORGE_CONVERSATION_MESSAGE_COUNT=$(prompt_forge_message_count_unstyled)
    # Update terminal title after variables are updated
    _forge_update_terminal_title
}

precmd_functions+=(update_forge_variables)