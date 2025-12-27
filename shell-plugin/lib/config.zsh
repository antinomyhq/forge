#!/usr/bin/env zsh

# Configuration variables for forge plugin
# Using typeset to keep variables local to plugin scope and prevent public exposure

typeset -h _FORGE_BIN="${FORGE_BIN:-forge}"
typeset -h _FORGE_CONVERSATION_PATTERN=":"
typeset -h _FORGE_MAX_COMMIT_DIFF="${FORGE_MAX_COMMIT_DIFF:-100000}"
typeset -h _FORGE_DELIMITER='\s\s+'
typeset -h _FORGE_PREVIEW_WINDOW="--preview-window=top:75%:wrap:border-sharp"

# Use ripgrep for file discovery
typeset -h _FORGE_RG_CMD="$(command -v rg 2>/dev/null || echo 'rg')"
# typeset -h _FORGE_FD_CMD="$(command -v fdfind 2>/dev/null || command -v fd 2>/dev/null || echo 'fd')"

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
