# Action handler: Display fish keyboard shortcuts
# Executes the forge binary's fish keyboard command
# Usage: _forge_action_keyboard

function _forge_action_keyboard
    echo
    $_FORGE_BIN fish keyboard
end
