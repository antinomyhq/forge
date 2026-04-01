# Default action handler: set active agent or execute command
# Port of _forge_action_default from shell-plugin/lib/dispatcher.zsh
#
# Flow:
# 1. Check if user_action is a CUSTOM command -> execute with `cmd` subcommand
# 2. If no input_text -> switch to agent (for AGENT type commands)
# 3. If input_text -> execute command with active agent context
#
# Usage: _forge_action_default <user_action> <input_text>

function _forge_action_default
    set -l user_action $argv[1]
    set -l input_text ""
    if test (count $argv) -ge 2
        set input_text $argv[2]
    end
    set -l command_type ""

    # Validate that the command exists in show-commands (if user_action is provided)
    if test -n "$user_action"
        set -l commands_list (_forge_get_commands)
        if test -n "$commands_list"
            # Check if the user_action is in the list of valid commands and extract the row
            set -l command_row (echo "$commands_list" | grep "^$user_action\b")
            if test -z "$command_row"
                echo
                _forge_log error "Command '"(set_color --bold)"$user_action"(set_color normal)"' not found"
                return 0
            end

            # Extract the command type from the second field (TYPE column)
            # Format: "COMMAND_NAME    TYPE    DESCRIPTION"
            set command_type (echo "$command_row" | awk '{print $2}')
            # Case-insensitive comparison
            if test (string lower -- "$command_type") = custom
                # Generate conversation ID if needed
                if test -z "$_FORGE_CONVERSATION_ID"
                    set -l new_id ($_FORGE_BIN conversation new)
                    set -g _FORGE_CONVERSATION_ID "$new_id"
                end

                echo
                # Execute custom command with execute subcommand
                if test -n "$input_text"
                    _forge_exec cmd execute --cid "$_FORGE_CONVERSATION_ID" "$user_action" "$input_text"
                else
                    _forge_exec cmd execute --cid "$_FORGE_CONVERSATION_ID" "$user_action"
                end
                return 0
            end
        end
    end

    # If input_text is empty, just set the active agent (only for AGENT type commands)
    if test -z "$input_text"
        if test -n "$user_action"
            if test (string lower -- "$command_type") != agent
                echo
                _forge_log error "Command '"(set_color --bold)"$user_action"(set_color normal)"' not found"
                return 0
            end
            echo
            # Set the agent in the global variable
            set -g _FORGE_ACTIVE_AGENT "$user_action"
            _forge_log info (set_color --bold white)(string upper -- "$_FORGE_ACTIVE_AGENT")(set_color normal)" "(set_color 888888)"is now the active agent"(set_color normal)
        end
        return 0
    end

    # Generate conversation ID if needed
    if test -z "$_FORGE_CONVERSATION_ID"
        set -l new_id ($_FORGE_BIN conversation new)
        set -g _FORGE_CONVERSATION_ID "$new_id"
    end

    echo

    # Only set the agent if user explicitly specified one
    if test -n "$user_action"
        set -g _FORGE_ACTIVE_AGENT "$user_action"
    end

    # Execute the forge command directly
    _forge_exec_interactive -p "$input_text" --cid "$_FORGE_CONVERSATION_ID"

    # Start background sync job if enabled and not already running
    _forge_start_background_sync
    # Start background update check
    _forge_start_background_update
end
