# Action handler: Dump conversation
# Port of _forge_action_dump from shell-plugin/lib/actions/core.zsh
#
# Dumps the current conversation content. Supports an "html" flag
# to export in HTML format.
#
# Usage: _forge_action_dump [html]

function _forge_action_dump
    set -l input_text ""
    if test (count $argv) -ge 1
        set input_text $argv[1]
    end

    if test "$input_text" = html
        _forge_handle_conversation_command dump --html
    else
        _forge_handle_conversation_command dump
    end
end
