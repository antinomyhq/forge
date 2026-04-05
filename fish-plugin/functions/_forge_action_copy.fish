# Action handler: Copy last assistant message to OS clipboard as raw markdown
# Port of _forge_action_copy from shell-plugin/lib/actions/conversation.zsh
#
# Fetches the last assistant message from the current conversation and
# copies it to the system clipboard using pbcopy (macOS), xclip, or xsel.
#
# Usage: _forge_action_copy

function _forge_action_copy
    echo

    if test -z "$_FORGE_CONVERSATION_ID"
        _forge_log error "No active conversation. Start a conversation first or use :conversation to see existing ones"
        return 0
    end

    # Fetch raw markdown from the last assistant message
    set -l content ($_FORGE_BIN conversation show --md "$_FORGE_CONVERSATION_ID" 2>/dev/null | string collect)

    if test -z "$content"
        _forge_log error "No assistant message found in the current conversation"
        return 0
    end

    # Copy to clipboard (pbcopy on macOS, xclip/xsel on Linux)
    if command -q pbcopy
        echo -n "$content" | pbcopy
    else if command -q xclip
        echo -n "$content" | xclip -selection clipboard
    else if command -q xsel
        echo -n "$content" | xsel --clipboard --input
    else
        _forge_log error "No clipboard utility found (pbcopy, xclip, or xsel required)"
        return 0
    end

    # Count lines and bytes for the confirmation message
    set -l line_count (echo "$content" | wc -l | string trim)
    set -l byte_count (echo -n "$content" | wc -c | string trim)

    _forge_log success "Copied to clipboard "(set_color 888888)"[$line_count lines, $byte_count bytes]"(set_color normal)
end
