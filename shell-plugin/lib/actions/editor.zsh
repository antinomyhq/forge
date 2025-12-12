#!/usr/bin/env zsh

# Editor and command suggestion action handlers

# Action handler: Open external editor for command composition
function _forge_action_editor() {
    local initial_text="$1"
    echo
    
    # Determine editor in order of preference: FORGE_EDITOR > EDITOR > nano
    local editor_cmd="${FORGE_EDITOR:-${EDITOR:-nano}}"
    
    # Validate editor exists
    if ! command -v "${editor_cmd%% *}" &>/dev/null; then
        _forge_log error "Editor not found: $editor_cmd (set FORGE_EDITOR or EDITOR)"
        _forge_reset
        return 1
    fi
    
    # Create .forge directory if it doesn't exist
    local forge_dir=".forge"
    if [[ ! -d "$forge_dir" ]]; then
        mkdir -p "$forge_dir" || {
            _forge_log error "Failed to create .forge directory"
            _forge_reset
            return 1
        }
    fi
    
    # Create temporary file with git-like naming: FORGE_EDITMSG.md
    local temp_file="${forge_dir}/FORGE_EDITMSG.md"
    touch "$temp_file" || {
        _forge_log error "Failed to create temporary file"
        _forge_reset
        return 1
    }
    
    # Ensure cleanup on exit
    trap "rm -f '$temp_file'" EXIT INT TERM
    
    # Pre-populate with initial text if provided
    if [[ -n "$initial_text" ]]; then
        echo "$initial_text" > "$temp_file"
    fi
    
    # Open editor
    eval "$editor_cmd '$temp_file'"
    local editor_exit_code=$?
    
    if [ $editor_exit_code -ne 0 ]; then
        _forge_log error "Editor exited with error code $editor_exit_code"
        _forge_reset
        return 1
    fi
    
    # Read and process content
    local content
    content=$(cat "$temp_file" | tr -d '\r')
    
    if [ -z "$content" ]; then
        _forge_log info "Editor closed with no content"
        _forge_reset
        return 0
    fi
    
    # Insert into buffer with : prefix
    BUFFER=": $content"
    CURSOR=${#BUFFER}
    
    _forge_log info "Command ready - press Enter to execute"
    # Only reset prompt if ZLE is available
    if {
        [[ $- == *i* ]] &&
        autoload -Uz zle 2>/dev/null &&
        zle 2>/dev/null
    }; then
        zle reset-prompt 2>/dev/null || true
    fi
}

# Action handler: Generate shell command from natural language
# Usage: :? <description>
function _forge_action_suggest() {
    local description="$1"
    
    if [[ -z "$description" ]]; then
        _forge_log error "Please provide a command description"
        _forge_reset
        return 0
    fi
    
    echo
    # Generate the command
    local generated_command
    generated_command=$(FORCE_COLOR=true CLICOLOR_FORCE=1 _forge_exec suggest "$description")
    
    if [[ -n "$generated_command" ]]; then
        # Replace the buffer with generated command
        BUFFER="$generated_command"
        CURSOR=${#BUFFER}
        # Only reset prompt if ZLE is available
        if {
            [[ $- == *i* ]] &&
            autoload -Uz zle 2>/dev/null &&
            zle 2>/dev/null
        }; then
            zle reset-prompt 2>/dev/null || true
        fi
    else
        _forge_log error "Failed to generate command"
        _forge_reset
    fi
}
