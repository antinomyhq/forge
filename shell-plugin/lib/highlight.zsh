#!/usr/bin/env zsh

# Syntax highlighting configuration for forge commands
# Style the conversation pattern with appropriate highlighting
# Keywords in yellow, rest in default white
#
# Use typeset -g to ensure we modify the global arrays even when
# sourced from within a function (lazy-loading plugin managers).

typeset -ga ZSH_HIGHLIGHT_PATTERNS
typeset -ga ZSH_HIGHLIGHT_HIGHLIGHTERS

# Style tagged files
ZSH_HIGHLIGHT_PATTERNS+=('@\[[^]]#\]' 'fg=cyan,bold')

# Highlight colon + command name (supports letters, numbers, hyphens, underscores) in yellow
ZSH_HIGHLIGHT_PATTERNS+=('(#s):[a-zA-Z0-9_-]#' 'fg=yellow,bold')

# Highlight everything after the command name + space in white
ZSH_HIGHLIGHT_PATTERNS+=('(#s):[a-zA-Z0-9_-]# [[:graph:]]*' 'fg=white')

ZSH_HIGHLIGHT_HIGHLIGHTERS+=(pattern)
