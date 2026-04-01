# Wrapper around fzf with consistent options for a unified UX
# All forge fzf invocations go through this function to ensure
# consistent appearance and behavior.
# Usage: _forge_fzf [additional fzf options...]

function _forge_fzf
    fzf --reverse --exact --cycle --select-1 --height 80% --no-scrollbar --ansi --color="header:bold" $argv
end
