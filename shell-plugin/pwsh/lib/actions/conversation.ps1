# Forge PowerShell Plugin - Conversation Actions

function Invoke-ForgeActionConversation {
    param([string]$InputText)

    # Toggle to previous conversation
    if ($InputText -eq '-') {
        if ($script:ForgePreviousConversationId) {
            Switch-ForgeConversation $script:ForgePreviousConversationId
            Write-ForgeLog 'success' "Switched to conversation: $script:ForgeConversationId"
            Invoke-ForgeExec info
        } else {
            Write-ForgeLog 'warning' 'No previous conversation'
        }
        return
    }

    # Direct ID switch
    if ($InputText) {
        Switch-ForgeConversation $InputText
        Write-ForgeLog 'success' "Switched to conversation: $InputText"
        Invoke-ForgeExec info
        return
    }

    # Interactive selection
    $conversations = & $script:ForgeBin list conversations --porcelain 2>$null
    if (-not $conversations) {
        Write-ForgeLog 'warning' 'No conversations found'
        return
    }

    if (Get-Command fzf -ErrorAction SilentlyContinue) {
        $selected = $conversations |
            Where-Object { $_ -match '\S' } |
            Select-Object -Skip 1 |
            Invoke-ForgeFzf -FzfArgs @("--prompt=Conversation > ")

        if ($selected) {
            $convId = ($selected -split '\s{2,}')[0].Trim()
            Switch-ForgeConversation $convId
            Write-ForgeLog 'success' "Switched to conversation: $convId"
            Invoke-ForgeExec info
        }
    } else {
        $conversations | Write-Host
    }
}

function Invoke-ForgeActionClone {
    param([string]$InputText)

    $sourceId = if ($InputText) { $InputText } else { $script:ForgeConversationId }

    if (-not $sourceId) {
        Write-ForgeLog 'error' 'No conversation to clone. Start or switch to one first.'
        return
    }

    $result = & $script:ForgeBin conversation clone $sourceId 2>$null
    if ($result) {
        $newId = ($result | Select-String -Pattern '[0-9a-f-]{36}').Matches.Value
        if ($newId) {
            Switch-ForgeConversation $newId
            Write-ForgeLog 'success' "Cloned conversation to: $newId"
            Invoke-ForgeExec info
        }
    }
}

function Invoke-ForgeActionCopy {
    $output = Invoke-ForgeExec dump --raw 2>$null
    if ($output) {
        $output | Set-Clipboard
        Write-ForgeLog 'success' 'Last assistant message copied to clipboard'
    } else {
        Write-ForgeLog 'warning' 'No content to copy'
    }
}

function Invoke-ForgeActionRename {
    param([string]$InputText)

    if (-not $script:ForgeConversationId) {
        Write-ForgeLog 'error' 'No active conversation to rename'
        return
    }

    if (-not $InputText) {
        Write-ForgeLog 'error' 'Usage: :rename <new-name>'
        return
    }

    Invoke-ForgeExec conversation rename $script:ForgeConversationId $InputText
    Write-ForgeLog 'success' "Conversation renamed to: $InputText"
}

function Invoke-ForgeActionConversationRename {
    param([string]$InputText)
    Invoke-ForgeActionRename $InputText
}
