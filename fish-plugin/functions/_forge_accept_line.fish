# Core dispatcher for forge :commands
# Port of forge-accept-line ZLE widget from shell-plugin/lib/dispatcher.zsh
#
# Parses the command line buffer for :command patterns and dispatches to
# the appropriate action handler. Non-: commands are executed normally.
#
# Patterns:
#   :command [args]  - dispatch to _forge_action_<command>
#   : <text>         - default action (send text to active agent)
#   anything else    - normal shell command execution
#
# Bound to Enter via conf.d/forge.fish

function _forge_accept_line
    # Grab the current command line buffer
    set -l buf (commandline)

    # --- Pattern 1: :command [args] ---
    set -l captures (string match --regex '^:([a-zA-Z][a-zA-Z0-9_-]*)( (.*))?$' -- "$buf")
    if test (count $captures) -ge 2
        set -l user_action $captures[2]  # group 1: command name
        set -l input_text ""
        if test (count $captures) -ge 4
            set input_text $captures[4]  # group 3: args after space
        end

        # Add to history before transforming
        builtin history add -- "$buf"

        # Handle aliases
        switch $user_action
            case ask
                set user_action sage
            case plan
                set user_action muse
        end

        # Dispatch to action handlers
        # IMPORTANT: When adding a new command here, also update
        #   crates/forge_main/src/built_in_commands.json
        switch $user_action
            case new n
                _forge_action_new "$input_text"
            case info i
                _forge_action_info
            case env e
                _forge_action_env
            case dump d
                _forge_action_dump "$input_text"
            case compact
                _forge_action_compact
            case retry r
                _forge_action_retry
            case agent a
                _forge_action_agent "$input_text"
            case conversation c
                _forge_action_conversation "$input_text"
            case config-provider provider p
                _forge_action_provider "$input_text"
            case config-model cm
                _forge_action_model "$input_text"
            case model m
                _forge_action_session_model "$input_text"
            case model-reset mr
                _forge_action_model_reset
            case config-commit-model ccm
                _forge_action_commit_model "$input_text"
            case config-suggest-model csm
                _forge_action_suggest_model "$input_text"
            case tools t
                _forge_action_tools
            case config
                _forge_action_config
            case config-edit ce
                _forge_action_config_edit
            case skill
                _forge_action_skill
            case edit ed
                _forge_action_editor "$input_text"
                # Editor intentionally modifies the buffer and handles its own repaint
                return
            case commit
                _forge_action_commit "$input_text"
            case commit-preview
                _forge_action_commit_preview "$input_text"
                # Commit-preview intentionally modifies the buffer
                return
            case suggest s
                _forge_action_suggest "$input_text"
                # Suggest intentionally modifies the buffer
                return
            case clone
                _forge_action_clone "$input_text"
            case rename rn
                _forge_action_rename "$input_text"
            case conversation-rename
                _forge_action_conversation_rename "$input_text"
            case copy
                _forge_action_copy
            case workspace-sync sync
                _forge_action_sync
            case workspace-init sync-init
                _forge_action_sync_init
            case workspace-status sync-status
                _forge_action_sync_status
            case workspace-info sync-info
                _forge_action_sync_info
            case provider-login login
                _forge_action_login "$input_text"
            case logout
                _forge_action_logout "$input_text"
            case doctor
                _forge_action_doctor
            case keyboard-shortcuts kb
                _forge_action_keyboard
            case '*'
                _forge_action_default "$user_action" "$input_text"
        end

        # Centralized reset after all actions complete
        _forge_reset
        return
    end

    # --- Pattern 2: ": text" (default action with args) ---
    set -l captures2 (string match --regex '^: (.*)$' -- "$buf")
    if test (count $captures2) -ge 2
        set -l input_text $captures2[2]  # group 1: text after ": "

        # Add to history before transforming
        builtin history add -- "$buf"

        _forge_action_default "" "$input_text"

        _forge_reset
        return
    end

    # --- Normal command: pass through to shell ---
    commandline -f execute
end
