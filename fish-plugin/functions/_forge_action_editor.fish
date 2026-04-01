# Action handler: Open external editor for command composition
# Port of _forge_action_editor from shell-plugin/lib/actions/editor.zsh
#
# Opens the user's preferred editor (FORGE_EDITOR > EDITOR > nano) with
# a temporary file. On save, loads the content back into the command line
# buffer prefixed with ": " so it can be dispatched as a forge command.
#
# Usage: _forge_action_editor [initial_text]

function _forge_action_editor
    set -l initial_text ""
    if test (count $argv) -ge 1
        set initial_text $argv[1]
    end
    echo

    # Determine editor in order of preference: FORGE_EDITOR > EDITOR > nano
    set -l editor_cmd
    if test -n "$FORGE_EDITOR"
        set editor_cmd "$FORGE_EDITOR"
    else if test -n "$EDITOR"
        set editor_cmd "$EDITOR"
    else
        set editor_cmd nano
    end

    # Validate editor exists (extract first word for commands with args)
    set -l editor_bin (string split ' ' -- "$editor_cmd")[1]
    if not command -q "$editor_bin"
        _forge_log error "Editor not found: $editor_cmd (set FORGE_EDITOR or EDITOR)"
        return 1
    end

    # Create .forge directory if it doesn't exist
    set -l forge_dir .forge
    if not test -d "$forge_dir"
        mkdir -p "$forge_dir"
        or begin
            _forge_log error "Failed to create .forge directory"
            return 1
        end
    end

    # Create temporary file with git-like naming: FORGE_EDITMSG.md
    set -l temp_file "$forge_dir/FORGE_EDITMSG.md"
    touch "$temp_file"
    or begin
        _forge_log error "Failed to create temporary file"
        return 1
    end

    # Pre-populate with initial text if provided
    if test -n "$initial_text"
        echo "$initial_text" > "$temp_file"
    end

    # Open editor with its own TTY session
    eval "$editor_cmd '$temp_file'" </dev/tty >/dev/tty 2>&1
    set -l editor_exit_code $status

    if test $editor_exit_code -ne 0
        _forge_log error "Editor exited with error code $editor_exit_code"
        # Clean up temp file
        rm -f "$temp_file"
        _forge_reset
        return 1
    end

    # Read and process content
    set -l content (cat "$temp_file" | tr -d '\r' | string collect)

    # Clean up temp file
    rm -f "$temp_file"

    if test -z "$content"
        _forge_log info "Editor closed with no content"
        commandline -r ""
        commandline -f repaint
        return 0
    end

    # Insert into buffer with : prefix
    commandline -r ": $content"
    commandline -f repaint
end
