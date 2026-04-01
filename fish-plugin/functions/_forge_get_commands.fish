# Lazy loader for forge commands cache
# Loads the commands list only when first needed, avoiding startup cost.
# Caches result in $_FORGE_COMMANDS global variable.
# Usage: _forge_get_commands
# Output: porcelain commands list on stdout

function _forge_get_commands
    if test -z "$_FORGE_COMMANDS"
        set -g _FORGE_COMMANDS (CLICOLOR_FORCE=0 $_FORGE_BIN list commands --porcelain 2>/dev/null | string collect)
    end
    echo "$_FORGE_COMMANDS"
end
