#!/usr/bin/env zsh

# Key bindings and widget registration for forge plugin

# Register ZLE widgets
zle -N forge-accept-line
zle -N forge-completion

# Custom bracketed-paste handler that wraps dropped file paths in @[] syntax
# and fixes syntax highlighting after paste
function forge-bracketed-paste() {
    # Capture the cursor position before the paste to isolate the pasted text
    local pre_cursor=$CURSOR
    local pre_lbuffer="$LBUFFER"
    
    # Call the built-in bracketed-paste widget first
    zle .$WIDGET "$@"
    
    # Extract the text that was actually pasted by comparing before/after state
    local pasted="${LBUFFER#$pre_lbuffer}"
    
    # Strip surrounding whitespace and quotes that terminals may add when
    # dragging and dropping file paths
    local trimmed="${pasted##[[:space:]]}"
    trimmed="${trimmed%%[[:space:]]}"
    # Remove surrounding single or double quotes (e.g. iTerm2 wraps paths with spaces)
    local sq="'"
    local dq='"'
    if [[ "$trimmed" == ${sq}*${sq} ]]; then
        trimmed="${trimmed#${sq}}"
        trimmed="${trimmed%${sq}}"
    elif [[ "$trimmed" == ${dq}*${dq} ]]; then
        trimmed="${trimmed#${dq}}"
        trimmed="${trimmed%${dq}}"
    fi
    
    # Check if the pasted text looks like a single file path that exists on disk
    # and is not already wrapped in @[...]
    if [[ -n "$trimmed" && -f "$trimmed" && "$pre_lbuffer" != *"@[" ]]; then
        # Replace the pasted text with the @[...] wrapped version
        LBUFFER="${pre_lbuffer}@[${trimmed}]"
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
