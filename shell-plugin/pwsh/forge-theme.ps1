# Forge PowerShell Theme - Custom Prompt
# Displays forge info right-aligned on the prompt line (like zsh RPROMPT).

function global:prompt {
    $forgeBin = if ($env:FORGE_BIN) { $env:FORGE_BIN } else { "forge" }

    # Pass session state to the rprompt command via env vars
    if ($script:ForgeConversationId) {
        $env:_FORGE_CONVERSATION_ID = $script:ForgeConversationId
    }
    if ($script:ForgeActiveAgent) {
        $env:_FORGE_ACTIVE_AGENT = $script:ForgeActiveAgent
    }
    if ($script:ForgeSessionModel) {
        $env:FORGE_SESSION__MODEL_ID = $script:ForgeSessionModel
    }
    if ($script:ForgeSessionProvider) {
        $env:FORGE_SESSION__PROVIDER_ID = $script:ForgeSessionProvider
    }

    $forgePrompt = ""
    try {
        $forgePrompt = & $forgeBin powershell rprompt 2>$null
        if ($forgePrompt -is [array]) {
            $forgePrompt = $forgePrompt -join ""
        }
    } catch {}

    # Build left prompt
    $leftPrompt = "PS $($executionContext.SessionState.Path.CurrentLocation)"

    if ($forgePrompt) {
        # Strip ANSI escapes to measure visible length of forge info
        $esc = [char]0x1b
        $plainForge = $forgePrompt -replace "$esc\[[0-9;]*m", ''
        $plainLeft = $leftPrompt

        $width = $Host.UI.RawUI.WindowSize.Width
        $gap = $width - $plainLeft.Length - $plainForge.Length

        if ($gap -gt 0) {
            # Right-align: left prompt + spaces + forge info
            Write-Host "$leftPrompt$(' ' * $gap)$forgePrompt"
        } else {
            # Terminal too narrow — put forge info on its own line
            Write-Host "$forgePrompt"
            Write-Host $leftPrompt -NoNewline
            return "> "
        }
    } else {
        Write-Host $leftPrompt
    }

    return "> "
}
