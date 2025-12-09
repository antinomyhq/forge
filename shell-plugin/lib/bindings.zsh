#!/usr/bin/env zsh

# Key bindings and widget registration for forge plugin

# Register ZLE widgets
zle -N forge-accept-line
zle -N forge-completion

# Custom bracketed-paste handler to fix syntax highlighting after paste
function forge-bracketed-paste() {
    zle .$WIDGET "$@"
    zle reset-prompt
}

# Register the bracketed paste widget to fix highlighting on paste
zle -N bracketed-paste forge-bracketed-paste

# Bind Enter to our custom accept-line that transforms :commands
bindkey '^M' forge-accept-line
bindkey '^J' forge-accept-line
# Update the Tab binding to use the new completion widget
bindkey '^I' forge-completion  # Tab for both @ and :command completion
