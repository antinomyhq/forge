# Clear the command line buffer and repaint the prompt
# Fish equivalent of the zsh _forge_reset which clears BUFFER and
# calls zle reset-prompt.
# Usage: _forge_reset

function _forge_reset
    commandline -r ""
    commandline -f repaint
end
