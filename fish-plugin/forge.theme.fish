# Forge theme for fish shell
# Defines fish_right_prompt to display AI context (model, agent, token count)
# Equivalent of shell-plugin/forge.theme.zsh

# Guard against double-loading
if set -q _FORGE_THEME_LOADED
    return
end

# Define right prompt function that calls _forge_prompt_info
# Fish automatically calls fish_right_prompt to render the right side
function fish_right_prompt
    _forge_prompt_info
end

# Mark theme as loaded
set -g _FORGE_THEME_LOADED 1
