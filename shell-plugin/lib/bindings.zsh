#!/usr/bin/env zsh

# Key bindings and widget registration for forge plugin

# Register ZLE widgets (only when ZLE is available and functional)
if {
    [[ $- == *i* ]] &&  # Interactive shell
    autoload -Uz zle 2>/dev/null &&  # ZLE can be loaded
    zle 2>/dev/null  # ZLE is functional
}; then
    zle -N forge-accept-line 2>/dev/null
    zle -N forge-completion 2>/dev/null

    # Custom bracketed-paste handler to fix syntax highlighting after paste
    function forge-bracketed-paste() {
        zle .$WIDGET "$@" 2>/dev/null || true
        zle reset-prompt 2>/dev/null || true
    }

    # Register the bracketed paste widget to fix highlighting on paste
    zle -N bracketed-paste forge-bracketed-paste 2>/dev/null

    # Bind Enter to our custom accept-line that transforms :commands
    bindkey '^M' forge-accept-line 2>/dev/null
    bindkey '^J' forge-accept-line 2>/dev/null
    # Update the Tab binding to use the new completion widget
    bindkey '^I' forge-completion 2>/dev/null  # Tab for both @ and :command completion
fi
