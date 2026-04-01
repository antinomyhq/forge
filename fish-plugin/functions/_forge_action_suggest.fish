# Action handler: Generate shell command from natural language
# Port of _forge_action_suggest from shell-plugin/lib/actions/editor.zsh
#
# Takes a natural language description and generates a shell command,
# loading it into the command line buffer for the user to review and execute.
#
# Usage: _forge_action_suggest <description>

function _forge_action_suggest
    set -l description ""
    if test (count $argv) -ge 1
        set description $argv[1]
    end

    if test -z "$description"
        _forge_log error "Please provide a command description"
        return 0
    end

    echo

    # Generate the command
    set -lx FORCE_COLOR true
    set -lx CLICOLOR_FORCE 1
    set -l generated_command (_forge_exec suggest "$description" | string collect)

    if test -n "$generated_command"
        # Replace the buffer with the generated command
        commandline -r "$generated_command"
        commandline -f repaint
    else
        _forge_log error "Failed to generate command"
    end
end
