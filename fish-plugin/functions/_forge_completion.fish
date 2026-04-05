# Custom completion handler for forge fish plugin
# Port of forge-completion ZLE widget from zsh.
# Handles @file completion, :command completion, and normal fish completion.
# Bound to Tab in conf.d/forge.fish.
# Usage: bound via `bind \t _forge_completion`

function _forge_completion
    # Get the full buffer and cursor position
    set -l buf (commandline)
    set -l cursor_pos (commandline -C)

    # Get text before cursor
    set -l lbuffer (string sub -l $cursor_pos -- "$buf")

    # Get current word (last space-separated token before cursor)
    set -l current_word (string match -r '[^ ]*$' -- "$lbuffer")

    # Handle @ completion (files and directories)
    if string match -rq '^@' -- "$current_word"
        set -l filter_text (string sub -s 2 -- "$current_word")
        set -l fzf_args \
            --preview="if [ -d {} ]; then ls -la {} 2>/dev/null; else $_FORGE_CAT_CMD {}; fi" \
            $_FORGE_PREVIEW_WINDOW

        set -l file_list ($_FORGE_FD_CMD --type f --type d --hidden --exclude .git | string collect)
        set -l selected
        if test -n "$filter_text"
            set selected (echo "$file_list" | _forge_fzf --query "$filter_text" $fzf_args)
        else
            set selected (echo "$file_list" | _forge_fzf $fzf_args)
        end

        if test -n "$selected"
            set selected "@[$selected]"
            # Replace current_word in lbuffer with the selection
            set -l prefix (string sub -l (math $cursor_pos - (string length -- "$current_word")) -- "$buf")
            set -l rbuffer (string sub -s (math $cursor_pos + 1) -- "$buf")
            set -l new_buf "$prefix$selected$rbuffer"
            commandline -r "$new_buf"
            commandline -C (math (string length -- "$prefix") + (string length -- "$selected"))
        end

        commandline -f repaint
        return 0
    end

    # Handle :command completion (supports letters, numbers, hyphens, underscores)
    if string match -rq '^:([a-zA-Z][a-zA-Z0-9_-]*)?$' -- "$lbuffer"
        # Extract the text after the colon for filtering
        set -l filter_text (string sub -s 2 -- "$lbuffer")

        # Lazily load the commands list
        set -l commands_list (_forge_get_commands)
        if test -n "$commands_list"
            # Use fzf for interactive selection with prefilled filter
            set -l selected
            if test -n "$filter_text"
                set selected (echo "$commands_list" | _forge_fzf --header-lines=1 --delimiter="$_FORGE_DELIMITER" --nth=1 --query "$filter_text" --prompt="Command > ")
            else
                set selected (echo "$commands_list" | _forge_fzf --header-lines=1 --delimiter="$_FORGE_DELIMITER" --nth=1 --prompt="Command > ")
            end

            if test -n "$selected"
                # Extract just the command name (first word before any description)
                set -l command_name (string split ' ' -- "$selected")[1]
                # Replace the current buffer with the selected command
                commandline -r ":$command_name "
                commandline -C (math 1 + (string length -- "$command_name") + 1)
            end
        end

        commandline -f repaint
        return 0
    end

    # Fall back to default fish completion
    commandline -f complete
end
