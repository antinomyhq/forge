# Action handler: Open the global forge config file in an editor
# Port of _forge_action_config_edit from zsh.
# Uses FORGE_EDITOR > EDITOR > nano
# Usage: _forge_action_config_edit

function _forge_action_config_edit
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

    # Validate editor exists (check first word of editor command)
    set -l editor_bin (string split ' ' -- "$editor_cmd")[1]
    if not command -q "$editor_bin"
        _forge_log error "Editor not found: $editor_cmd (set FORGE_EDITOR or EDITOR)"
        return 1
    end

    set -l config_file "$HOME/forge/.forge.toml"

    # Ensure the config directory exists
    if not test -d "$HOME/forge"
        mkdir -p "$HOME/forge"; or begin
            _forge_log error "Failed to create ~/forge directory"
            return 1
        end
    end

    # Create the config file if it does not yet exist
    if not test -f "$config_file"
        touch "$config_file"; or begin
            _forge_log error "Failed to create $config_file"
            return 1
        end
    end

    # Open editor with its own TTY session
    eval "$editor_cmd '$config_file'" </dev/tty >/dev/tty 2>&1
    set -l exit_code $status

    if test $exit_code -ne 0
        _forge_log error "Editor exited with error code $exit_code"
    end

    _forge_reset
end
