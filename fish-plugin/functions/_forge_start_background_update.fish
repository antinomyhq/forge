# Start background update check
# Mirrors the background sync pattern to silently check for and apply updates.
# Runs forge update --no-confirm in the background with all output suppressed.
# Usage: _forge_start_background_update

function _forge_start_background_update
    # Run update check in background with all output suppressed
    # Redirect stdin to /dev/null to prevent hanging
    $_FORGE_BIN update --no-confirm >/dev/null 2>&1 </dev/null &
    disown
end
