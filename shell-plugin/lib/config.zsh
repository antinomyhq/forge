#!/usr/bin/env zsh

# Configuration variables for forge plugin
# Using typeset to keep variables local to plugin scope and prevent public exposure

# Function to execute forge binary with proper word splitting
# Handles multi-word commands like "npx forgecode" correctly
function _forge_bin() {
    ${=FORGE_BIN:-forge} "$@"
}

typeset -h _FORGE_CONVERSATION_PATTERN=":"
typeset -h _FORGE_MAX_COMMIT_DIFF="${FORGE_MAX_COMMIT_DIFF:-100000}"
typeset -h _FORGE_DELIMITER='\s\s+'
typeset -h _FORGE_PREVIEW_WINDOW="--preview-window=top:75%:wrap:border-sharp"

# Detect fd command - Ubuntu/Debian use 'fdfind', others use 'fd'
typeset -h _FORGE_FD_CMD="$(command -v fdfind 2>/dev/null || command -v fd 2>/dev/null || echo 'fd')"

# Detect bat command - use bat if available, otherwise fall back to cat
if command -v bat &>/dev/null; then
    typeset -h _FORGE_CAT_CMD="bat --color=always --style=numbers,changes --line-range=:500"
else
    typeset -h _FORGE_CAT_CMD="cat"
fi

# Commands cache - loaded lazily on first use
typeset -h _FORGE_COMMANDS=""

# Hidden variables to be used only via the ForgeCLI
typeset -h _FORGE_CONVERSATION_ID
typeset -h _FORGE_ACTIVE_AGENT

# Previous conversation ID for :conversation - (like cd -)
typeset -h _FORGE_PREVIOUS_CONVERSATION_ID
