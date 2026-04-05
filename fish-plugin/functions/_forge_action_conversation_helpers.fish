# Conversation state management helpers
# Port of _forge_switch_conversation, _forge_clear_conversation,
# _forge_clone_and_switch from shell-plugin/lib/actions/conversation.zsh
#
# These helper functions manage conversation switching, clearing, and cloning.
# They track the previous conversation ID so that :conversation - works like cd -.

# Switch to a conversation and track previous (like cd -)
# Usage: _forge_switch_conversation <new_conversation_id>
function _forge_switch_conversation
    set -l new_conversation_id $argv[1]

    # Only update previous if we're switching to a different conversation
    if test -n "$_FORGE_CONVERSATION_ID"; and test "$_FORGE_CONVERSATION_ID" != "$new_conversation_id"
        # Save current as previous
        set -g _FORGE_PREVIOUS_CONVERSATION_ID "$_FORGE_CONVERSATION_ID"
    end

    # Set the new conversation as active
    set -g _FORGE_CONVERSATION_ID "$new_conversation_id"
end

# Reset/clear conversation and track previous (like cd -)
# Usage: _forge_clear_conversation
function _forge_clear_conversation
    # Save current as previous before clearing
    if test -n "$_FORGE_CONVERSATION_ID"
        set -g _FORGE_PREVIOUS_CONVERSATION_ID "$_FORGE_CONVERSATION_ID"
    end

    # Clear the current conversation
    set -g _FORGE_CONVERSATION_ID ""
end

# Clone a conversation and switch to the new clone
# Usage: _forge_clone_and_switch <clone_target>
function _forge_clone_and_switch
    set -l clone_target $argv[1]

    # Store original conversation ID to check if we're cloning current conversation
    set -l original_conversation_id "$_FORGE_CONVERSATION_ID"

    # Execute clone command
    _forge_log info "Cloning conversation "(set_color --bold)"$clone_target"(set_color normal)
    set -l clone_output ($_FORGE_BIN conversation clone "$clone_target" 2>&1 | string collect)
    set -l clone_exit_code $status

    if test $clone_exit_code -eq 0
        # Extract new conversation ID from output (UUID pattern)
        set -l new_id (echo "$clone_output" | grep -oE '[a-f0-9-]{36}' | tail -1)

        if test -n "$new_id"
            # Switch to cloned conversation and track previous
            _forge_switch_conversation "$new_id"

            _forge_log success "Switched to conversation "(set_color --bold)"$new_id"(set_color normal)

            # Show content and info only if cloning a different conversation (not current one)
            if test "$clone_target" != "$original_conversation_id"
                echo
                _forge_exec conversation show "$new_id"

                # Show new conversation info
                echo
                _forge_exec conversation info "$new_id"
            end
        else
            _forge_log error "Failed to extract new conversation ID from clone output"
        end
    else
        _forge_log error "Failed to clone conversation: $clone_output"
    end
end
