# !! Contents within this block are managed by 'forge fish setup' !!
# !! Do not edit manually - changes will be overwritten !!

# Load forge shell plugin (commands, completions, keybindings) if not already loaded
if not set -q _FORGE_PLUGIN_LOADED
    eval (forge fish plugin)
end

# Load forge shell theme (prompt with AI context) if not already loaded
if not set -q _FORGE_THEME_LOADED
    eval (forge fish theme)
end
