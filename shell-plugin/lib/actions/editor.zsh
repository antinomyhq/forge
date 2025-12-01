#!/usr/bin/env zsh

# Editor and command suggestion action handlers

# ZLE Widget: Open external editor for current buffer content
function forge-editor() {
    local current_buffer="$BUFFER"
    _forge_action_editor "$current_buffer"
    local editor_result=$?
    
    if [[ $editor_result -eq 0 ]]; then
        # Read the edited content back into the buffer
        if [[ -f ".forge/FORGE_EDITMSG" ]]; then
            local edited_content=$(cat ".forge/FORGE_EDITMSG")
            BUFFER="$edited_content"
            CURSOR=${#BUFFER}  # Move cursor to end
        fi
    fi
    
    # Reset prompt and redraw
    zle reset-prompt
}

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
    
    # Create temporary file with git-like naming: FORGE_EDITMSG
    local temp_file="${forge_dir}/FORGE_EDITMSG"
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
    _forge_launch_editor "$editor_cmd" "$temp_file"
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
    zle reset-prompt
}

# Helper function to check if editor requires TTY
_forge_editor_needs_tty() {
    local editor_cmd="$1"
    
    # Special handling for common editors
    case "${editor_cmd%% *}" in
        nano)
            # Nano generally works fine without TTY but benefits from it
            _forge_log debug "Editor: nano, returning 1 (no TTY needed)"
            return 1  # Doesn't need TTY
            ;;
        vim|vi|emacs)
            _forge_log debug "Editor: $editor_cmd, returning 0 (TTY needed)"
            return 0  # These definitely need TTY
            ;;
        *)
            _forge_log debug "Editor: $editor_cmd, testing with --version"
            # Quick test: try to get version - editors needing TTY often fail this
            ! timeout 1s "$editor_cmd" --version >/dev/null 2>&1
            ;;
    esac
}

# Helper function to launch editor with proper TTY handling
_forge_launch_editor() {
    local editor_cmd="$1"
    local temp_file="$2"
    
    # If we have TTY or editor doesn't need it, run normally
    if [[ -t 0 && -t 1 ]]; then
        # We have proper TTY, run normally
        "$editor_cmd" "$temp_file"
        return $?
    elif ! _forge_editor_needs_tty "${editor_cmd%% *}"; then
        # Editor doesn't need TTY but we don't have TTY - provide minimal TTY for safety
        _forge_log debug "Editor doesn't need TTY but no TTY available, providing minimal TTY"
        
        # Check if we can access /dev/tty for TTY allocation
        if [[ ! -r /dev/tty ]]; then
            _forge_log error "Cannot access TTY device for editor"
            return 1
        fi
        
        # For nano, use -q flag to ignore stdin errors but still provide TTY
        case "${editor_cmd%% *}" in
            nano)
                "$editor_cmd" -q "$temp_file" < /dev/tty > /dev/tty 2>&1
                ;;
            *)
                "$editor_cmd" "$temp_file" < /dev/tty > /dev/tty 2>&1
                ;;
        esac
        return $?
    fi
    
    # Check if we can access /dev/tty for TTY allocation
    if [[ ! -t 0 && ! -r /dev/tty ]]; then
        _forge_log error "Cannot access TTY device for editor"
        return 1
    fi
    
    # No TTY available but editor needs it - try multiple approaches
    if command -v script >/dev/null 2>&1; then
        # Method 1: Use script with proper terminal size preservation
        local lines=${LINES:-24}
        local columns=${COLUMNS:-80}
        
        # Try direct approach first (more reliable)
        _forge_log debug "Trying direct /dev/tty approach first"
        export LINES=$lines COLUMNS=$columns TERM=${TERM:-xterm-256color}
        stty rows $lines cols $columns -icanon -echo 2>/dev/null
        trap 'stty icanon echo 2>/dev/null' INT TERM
        
        # For nano, use -q flag to ignore stdin errors
        case "${editor_cmd%% *}" in
            nano)
                "$editor_cmd" -q "$temp_file" < /dev/tty > /dev/tty 2>&1
                ;;
            *)
                "$editor_cmd" "$temp_file" < /dev/tty > /dev/tty 2>&1
                ;;
        esac
        local script_result=$?
        stty icanon echo 2>/dev/null
        
        # If direct approach failed, try script
        if [ $script_result -ne 0 ]; then
            _forge_log debug "Direct approach failed, trying script"
            LINES=$lines COLUMNS=$columns stty rows $lines cols $columns 2>/dev/null && \
            # For nano, use -q flag to ignore stdin errors
            case "${editor_cmd%% *}" in
                nano)
                    script -q -c "export LINES=$lines COLUMNS=$columns TERM=${TERM:-xterm-256color}; stty rows $lines cols $columns -icanon -echo; trap 'stty icanon echo 2>/dev/null; exit' INT TERM; $editor_cmd -q $temp_file; stty icanon echo" /dev/null < /dev/tty > /dev/tty 2>&1
                    ;;
                *)
                    script -q -c "export LINES=$lines COLUMNS=$columns TERM=${TERM:-xterm-256color}; stty rows $lines cols $columns -icanon -echo; trap 'stty icanon echo 2>/dev/null; exit' INT TERM; $editor_cmd $temp_file; stty icanon echo" /dev/null < /dev/tty > /dev/tty 2>&1
                    ;;
            esac
            script_result=$?
        fi
        
        return $script_result
    elif command -v setsid >/dev/null 2>&1; then
        # Method 2: Try setsid to create new session
        _forge_log debug "Using setsid approach"
        local lines=${LINES:-24}
        local columns=${COLUMNS:-80}
        export LINES=$lines COLUMNS=$columns TERM=${TERM:-xterm-256color}
        stty rows $lines cols $columns -icanon -echo 2>/dev/null
        trap 'stty icanon echo 2>/dev/null' INT TERM
        
        # For nano, use -q flag to ignore stdin errors
        case "${editor_cmd%% *}" in
            nano)
                setsid -w "$editor_cmd" -q "$temp_file" < /dev/tty > /dev/tty 2>&1
                ;;
            *)
                setsid -w "$editor_cmd" "$temp_file" < /dev/tty > /dev/tty 2>&1
                ;;
        esac
        script_result=$?
        stty icanon echo 2>/dev/null
        
        return $script_result
    elif command -v unbuffer >/dev/null 2>&1; then
        # Method 3: Try unbuffer as alternative to script
        _forge_log debug "Using unbuffer as alternative to script"
        local lines=${LINES:-24}
        local columns=${COLUMNS:-80}
        LINES=$lines COLUMNS=$columns unbuffer -p "export LINES=$lines COLUMNS=$columns TERM=${TERM:-xterm-256color}; stty rows $lines cols $columns -icanon -echo; trap 'stty icanon echo 2>/dev/null; exit' INT TERM; $editor_cmd $temp_file; stty icanon echo" < /dev/tty > /dev/tty 2>&1
        script_result=$?
        
        return $script_result
    else
        # Method 4: Direct approach with /dev/tty (last resort)
        _forge_log debug "Using direct /dev/tty approach (last resort)"
        local lines=${LINES:-24}
        local columns=${COLUMNS:-80}
        export LINES=$lines COLUMNS=$columns TERM=${TERM:-xterm-256color}
        stty rows $lines cols $columns -icanon -echo 2>/dev/null
        trap 'stty icanon echo 2>/dev/null' INT TERM
        
        # For nano, use -q flag to ignore stdin errors
        case "${editor_cmd%% *}" in
            nano)
                "$editor_cmd" -q "$temp_file" < /dev/tty > /dev/tty 2>&1
                ;;
            *)
                "$editor_cmd" "$temp_file" < /dev/tty > /dev/tty 2>&1
                ;;
        esac
        stty icanon echo 2>/dev/null
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
        # Replace the buffer with the generated command
        BUFFER="$generated_command"
        CURSOR=${#BUFFER}
        zle reset-prompt
    else
        _forge_log error "Failed to generate command"
        _forge_reset
    fi
}