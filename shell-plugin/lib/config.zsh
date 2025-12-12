#!/usr/bin/env zsh

# Configuration variables for forge plugin
# Using typeset to keep variables local to plugin scope and prevent public exposure

typeset -h _FORGE_BIN="${FORGE_BIN:-forge}"
typeset -h _FORGE_CONVERSATION_PATTERN=":"
typeset -h _FORGE_MAX_COMMIT_DIFF="${FORGE_MAX_COMMIT_DIFF:-100000}"
typeset -h _FORGE_DELIMITER='\s\s+'
typeset -h _FORGE_PREVIEW_WINDOW="--preview-window=top:75%:wrap:border-sharp"

# Detect fd command - Ubuntu/Debian use 'fdfind', others use 'fd'
typeset -h _FORGE_FD_CMD="$(command -v fdfind 2>/dev/null || command -v fd 2>/dev/null || echo 'fd')"

# Log level configuration - controls message verbosity
# Default to "info" level to maintain backward compatibility while hiding debug messages
# Hierarchy: debug=0 < info=1 < warning=2 < error=3 < success=4
typeset -h _FORGE_LOG_LEVEL="${FORGE_LOG_LEVEL:-info}"

# Detect bat command - use bat if available, otherwise fall back to cat
if command -v bat &>/dev/null; then
    typeset -h _FORGE_CAT_CMD="bat --color=always --style=numbers,changes --line-range=:500"
else
    typeset -h _FORGE_CAT_CMD="cat"
fi

# Commands cache - loaded lazily on first use
typeset -h _FORGE_COMMANDS=""

# Store active agent ID in a local variable (session-scoped)
# Default to "forge" agent
export _FORGE_ACTIVE_AGENT=forge

# Store conversation ID in a temporary variable (local to plugin)
export _FORGE_CONVERSATION_ID=""
