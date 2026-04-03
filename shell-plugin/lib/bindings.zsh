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
    
    # Strip all surrounding whitespace that terminals may add
    local trimmed="${pasted##[[:space:]]#}"
    trimmed="${trimmed%%[[:space:]]#}"
    # Remove surrounding single or double quotes (e.g. iTerm2 wraps paths
    # with spaces in quotes when dragging and dropping)
    local sq="'"
    local dq='"'
    if [[ "$trimmed" == ${sq}*${sq} ]]; then
        trimmed="${trimmed#${sq}}"
        trimmed="${trimmed%${sq}}"
    elif [[ "$trimmed" == ${dq}*${dq} ]]; then
        trimmed="${trimmed#${dq}}"
        trimmed="${trimmed%${dq}}"
    fi
    # Un-escape backslash-escaped characters (e.g. Ghostty sends
    # /path/my\ folder/file.txt for paths with spaces).
    # Process character by character to remove escaping backslashes.
    local unescaped="$trimmed"
    if [[ "$unescaped" == *\\* ]]; then
        # Use printf to interpret backslash escapes, but we only want to
        # remove the escaping backslash before literal chars (especially
        # spaces).  Process character by character via parameter expansion.
        local tmp=""
        local i=1
        while (( i <= ${#unescaped} )); do
            if [[ "${unescaped[$i]}" == "\\" && $i -lt ${#unescaped} ]]; then
                (( i++ ))
                tmp+="${unescaped[$i]}"
            else
                tmp+="${unescaped[$i]}"
            fi
            (( i++ ))
        done
        unescaped="$tmp"
    fi
    
    # Only auto-wrap when the line is a forge command (starts with ':').
    # This avoids mangling paths pasted into normal shell commands like
    # 'vim /some/path' or 'cat /some/path'.
    # Try the cleaned path first, then fall back to the un-escaped version
    # for terminals that backslash-escape spaces in drag-and-drop.
    if [[ "$BUFFER" == :* && -n "$trimmed" && "$pre_lbuffer" != *"@[" ]]; then
        if [[ -f "$trimmed" ]]; then
            LBUFFER="${pre_lbuffer}@[${trimmed}]"
        elif [[ "$unescaped" != "$trimmed" && -f "$unescaped" ]]; then
            LBUFFER="${pre_lbuffer}@[${unescaped}]"
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
