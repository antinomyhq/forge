# Action handler: Run forge environment diagnostics
# Executes the forge binary's fish doctor command
# Usage: _forge_action_doctor

function _forge_action_doctor
    echo
    $_FORGE_BIN fish doctor
end
