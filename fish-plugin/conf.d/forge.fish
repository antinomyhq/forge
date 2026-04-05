# Configuration and initialization for forge fish plugin
# Auto-sourced by fish shell from conf.d/
# Equivalent of shell-plugin/lib/config.zsh + shell-plugin/lib/bindings.zsh

# Guard against double-loading
if set -q _FORGE_PLUGIN_LOADED
    return
end

# --- Configuration variables (equivalent of zsh typeset -h) ---

# Path to forge binary
if set -q FORGE_BIN; and test -n "$FORGE_BIN"
    set -g _FORGE_BIN "$FORGE_BIN"
else if command -q forge
    set -g _FORGE_BIN (command -v forge)
else
    set -g _FORGE_BIN "forge"
end

# Pattern used to identify conversation commands
set -g _FORGE_CONVERSATION_PATTERN ":"

# Delimiter for porcelain output parsing (multi-space)
set -g _FORGE_DELIMITER '\\s\\s+'

# Max diff size for commit operations
if set -q FORGE_MAX_COMMIT_DIFF; and test -n "$FORGE_MAX_COMMIT_DIFF"
    set -g _FORGE_MAX_COMMIT_DIFF "$FORGE_MAX_COMMIT_DIFF"
else
    set -g _FORGE_MAX_COMMIT_DIFF 100000
end

# fzf preview window options
set -g _FORGE_PREVIEW_WINDOW "--preview-window=bottom:75%:wrap:border-sharp"

# Detect fd command - Ubuntu/Debian use 'fdfind', others use 'fd'
if command -q fdfind
    set -g _FORGE_FD_CMD (command -v fdfind)
else if command -q fd
    set -g _FORGE_FD_CMD (command -v fd)
else
    set -g _FORGE_FD_CMD "fd"
end

# Detect bat command - use bat if available, otherwise fall back to cat
if command -q bat
    set -g _FORGE_CAT_CMD "bat --color=always --style=numbers,changes --line-range=:500"
else
    set -g _FORGE_CAT_CMD "cat"
end

# Commands cache - loaded lazily on first use
set -g _FORGE_COMMANDS ""

# Hidden variables used only via the ForgeCLI
set -g _FORGE_CONVERSATION_ID ""
set -g _FORGE_ACTIVE_AGENT ""

# Previous conversation ID for :conversation - (like cd -)
set -g _FORGE_PREVIOUS_CONVERSATION_ID ""

# Session-scoped model and provider overrides (set via :model / :m).
# When non-empty, these are passed as environment variables to every forge
# invocation for the lifetime of the current shell session.
set -g _FORGE_SESSION_MODEL ""
set -g _FORGE_SESSION_PROVIDER ""

# --- Key bindings (equivalent of shell-plugin/lib/bindings.zsh) ---

# Bind Enter to our custom accept-line that transforms :commands
bind \r _forge_accept_line
bind \n _forge_accept_line

# Bind Tab for :command and @ completion
bind \t _forge_completion

# Also bind in vi insert mode if available
if bind -M insert \r 2>/dev/null
    bind -M insert \r _forge_accept_line
    bind -M insert \n _forge_accept_line
    bind -M insert \t _forge_completion
end

# Mark plugin as loaded
set -g _FORGE_PLUGIN_LOADED 1
