#!/usr/bin/env zsh

# Enable prompt substitution for RPROMPT
setopt PROMPT_SUBST

# Model and agent info with token count
# Fully formatted output directly from Rust
# Returns ZSH-formatted string ready for use in RPROMPT
function _forge_prompt_info() {
    local forge_bin="${_FORGE_BIN:-${FORGE_BIN:-forge}}"
    
    # Get fully formatted prompt from forge (single command)
    _FORGE_CONVERSATION_ID=$_FORGE_CONVERSATION_ID _FORGE_ACTIVE_AGENT=$_FORGE_ACTIVE_AGENT "$forge_bin" zsh rprompt
}

# Right prompt: agent and model with token count (uses single forge prompt command)
# Set RPROMPT if empty, otherwise append to existing value
if [[ -z "$_FORGE_THEME_LOADED" ]]; then
    RPROMPT='$(_forge_prompt_info)'"${RPROMPT:+ ${RPROMPT}}"
fi
