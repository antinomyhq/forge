#!/usr/bin/env zsh

# Key bindings and widget registration for forge plugin

# Register ZLE widgets
zle -N forge-accept-line
zle -N forge-completion

# Custom bracketed-paste handler that wraps dropped file paths in @[] syntax
# and fixes syntax highlighting after paste.
#
# Path detection and wrapping is delegated to `forge zsh format` (Rust) so
# that all parsing logic lives in one well-tested place.
function forge-bracketed-paste() {
    # Capture the cursor position before the paste to isolate the pasted text
    local pre_cursor=$CURSOR
    local pre_lbuffer="$LBUFFER"
    
    # Call the built-in bracketed-paste widget first
    zle .$WIDGET "$@"
    
    # Extract the text that was actually pasted by comparing before/after state
    local pasted="${LBUFFER#$pre_lbuffer}"
    
    # Only auto-wrap when the line is a forge command (starts with ':').
    # This avoids mangling paths pasted into normal shell commands like
    # 'vim /some/path' or 'cat /some/path'.
    if [[ "$BUFFER" == :* && -n "$pasted" && "$pre_lbuffer" != *"@[" ]]; then
        local formatted=$("$_FORGE_BIN" zsh format --buffer "$pasted")
        if [[ -n "$formatted" && "$formatted" != "$pasted" ]]; then
            LBUFFER="${pre_lbuffer}${formatted}"
        fi
    fi
    
    # Explicitly redisplay the buffer to ensure paste content is visible
    # This is critical for large or multiline pastes
    zle redisplay
    
    # Reset the prompt to trigger syntax highlighting refresh
    # The redisplay before reset-prompt ensures the buffer is fully rendered
    zle reset-prompt
}

# Register the bracketed paste widget to fix highlighting on paste
zle -N bracketed-paste forge-bracketed-paste

# Bind Enter to our custom accept-line that transforms :commands
bindkey '^M' forge-accept-line
bindkey '^J' forge-accept-line
# Update the Tab binding to use the new completion widget
bindkey '^I' forge-completion  # Tab for both @ and :command completion
