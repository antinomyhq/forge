# Forge PowerShell Plugin - Core Actions

function Invoke-ForgeActionNew {
    param([string]$InputText)
    Clear-ForgeConversation
    $script:ForgeCommands = $null  # Reset commands cache
    Write-ForgeLog 'success' 'New conversation started'
    if ($InputText) {
        Invoke-ForgeExec $InputText
    }
}

function Invoke-ForgeActionInfo {
    $info = @()
    $info += "Agent:        $(if ($script:ForgeActiveAgent) { $script:ForgeActiveAgent } else { 'forge' })"
    $info += "Conversation: $(if ($script:ForgeConversationId) { $script:ForgeConversationId } else { '(none)' })"
    if ($script:ForgeSessionModel) {
        $info += "Session Model: $script:ForgeSessionModel"
    }
    if ($script:ForgeSessionProvider) {
        $info += "Session Provider: $script:ForgeSessionProvider"
    }
    if ($script:ForgeSessionReasoningEffort) {
        $info += "Reasoning Effort: $script:ForgeSessionReasoningEffort"
    }
    Invoke-ForgeExec info
}

function Invoke-ForgeActionEnv {
    Write-Host "FORGE_BIN=$script:ForgeBin"
    Write-Host "_FORGE_CONVERSATION_ID=$script:ForgeConversationId"
    Write-Host "_FORGE_ACTIVE_AGENT=$script:ForgeActiveAgent"
    Write-Host "FORGE_SESSION__MODEL_ID=$script:ForgeSessionModel"
    Write-Host "FORGE_SESSION__PROVIDER_ID=$script:ForgeSessionProvider"
    Write-Host "FORGE_REASONING__EFFORT=$script:ForgeSessionReasoningEffort"
}

function Invoke-ForgeActionDump {
    param([string]$InputText)
    if ($InputText -eq '--html' -or $InputText -eq 'html') {
        Invoke-ForgeExec dump --html
    } else {
        Invoke-ForgeExec dump
    }
}

function Invoke-ForgeActionCompact {
    Invoke-ForgeExec compact
}

function Invoke-ForgeActionRetry {
    Invoke-ForgeExec retry
}
