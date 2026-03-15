#!/usr/bin/env zsh

# Enable prompt expansion for command substitution and percent escapes in RPROMPT.
# PROMPT_PERCENT is required when users or frameworks disabled percent prompt
# expansion globally; otherwise Forge's %B/%F{...}/%f/%b sequences render
# literally instead of being interpreted by zsh.
setopt PROMPT_SUBST PROMPT_PERCENT

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
