# Forge PowerShell Plugin - Command Dispatcher
# Intercepts Enter key to detect and route :command patterns via PSReadLine.

function Invoke-ForgeChat {
    param([string]$InputText)

    if (-not $InputText) { return }

    # Generate conversation ID if needed
    if (-not $script:ForgeConversationId) {
        $newId = & $script:ForgeBin conversation new 2>$null
        if ($newId) {
            $newId = $newId.Trim()
            $script:ForgeConversationId = $newId
            $env:_FORGE_CONVERSATION_ID = $newId
        }
    }

    # Build the command: forge --agent <agent> -p "text" --cid <cid>
    $agentId = if ($script:ForgeActiveAgent) { $script:ForgeActiveAgent } else { "forge" }
    $cmd = @("--agent", $agentId, "-p", $InputText)

    if ($script:ForgeConversationId) {
        $cmd += @("--cid", $script:ForgeConversationId)
    }

    # Set session env vars
    if ($script:ForgeSessionModel) {
        $env:FORGE_SESSION__MODEL_ID = $script:ForgeSessionModel
    }
    if ($script:ForgeSessionProvider) {
        $env:FORGE_SESSION__PROVIDER_ID = $script:ForgeSessionProvider
    }
    if ($script:ForgeSessionReasoningEffort) {
        $env:FORGE_REASONING__EFFORT = $script:ForgeSessionReasoningEffort
    }

    & $script:ForgeBin @cmd
}

function Invoke-ForgeDispatch {
    param(
        [string]$Action,
        [string]$InputText
    )

    # Default action (no command name, just ": text")
    if (-not $Action) {
        if ($InputText) {
            Invoke-ForgeChat $InputText
        }
        return
    }

    # Resolve aliases
    $resolvedAction = switch ($Action) {
        'n'    { 'new' }
        'i'    { 'info' }
        'a'    { 'agent' }
        'c'    { 'conversation' }
        'm'    { 'session-model' }
        'cm'   { 'config-model' }
        're'   { 'reasoning-effort' }
        'ce'   { 'config-edit' }
        'kb'   { 'keyboard-shortcuts' }
        's'    { 'suggest' }
        'ask'  { 'sage' }
        'plan' { 'muse' }
        default { $Action }
    }

    switch ($resolvedAction) {
        # Core actions
        'new'              { Invoke-ForgeActionNew $InputText }
        'info'             { Invoke-ForgeActionInfo }
        'env'              { Invoke-ForgeActionEnv }
        'dump'             { Invoke-ForgeActionDump $InputText }
        'compact'          { Invoke-ForgeActionCompact }
        'retry'            { Invoke-ForgeActionRetry }

        # Config actions
        'agent'            { Invoke-ForgeActionAgent $InputText }
        'model'            { Invoke-ForgeActionConfigModel $InputText }
        'session-model'    { Invoke-ForgeActionSessionModel $InputText }
        'config-model'     { Invoke-ForgeActionConfigModel $InputText }
        'commit-model'     { Invoke-ForgeActionCommitModel $InputText }
        'suggest-model'    { Invoke-ForgeActionSuggestModel $InputText }
        'reasoning-effort' { Invoke-ForgeActionReasoningEffort $InputText }
        'config-reasoning-effort' { Invoke-ForgeActionConfigReasoningEffort $InputText }
        'config'           { Invoke-ForgeActionConfig }
        'config-edit'      { Invoke-ForgeActionConfigEdit }
        'config-reload'    { Invoke-ForgeActionConfigReload }
        'tools'            { Invoke-ForgeActionTools }
        'skill'            { Invoke-ForgeActionSkills }
        'sync'             { Invoke-ForgeActionSync }

        # Conversation actions
        'conversation'     { Invoke-ForgeActionConversation $InputText }
        'clone'            { Invoke-ForgeActionClone $InputText }
        'copy'             { Invoke-ForgeActionCopy }
        'rename'           { Invoke-ForgeActionRename $InputText }
        'conversation-rename' { Invoke-ForgeActionConversationRename $InputText }

        # Git actions
        'commit'           { Invoke-ForgeActionCommit $InputText }
        'commit-preview'   { Invoke-ForgeActionCommitPreview $InputText }
        'suggest'          { Invoke-ForgeActionSuggest $InputText }

        # Auth actions
        'provider-login'   { Invoke-ForgeActionLogin }
        'login'            { Invoke-ForgeActionLogin }
        'logout'           { Invoke-ForgeActionLogout }

        # Editor
        'edit'             { Invoke-ForgeActionEditor }

        # Diagnostics
        'doctor'           { Invoke-ForgeActionDoctor }
        'keyboard-shortcuts' { Invoke-ForgeActionKeyboard }

        # Provider
        'provider'         { Invoke-ForgeActionProvider $InputText }
        'config-provider'  { Invoke-ForgeActionProvider $InputText }

        default {
            # Check if it's a known agent name — set it and chat
            $commands = Get-ForgeCommands
            $isAgent = $false
            if ($commands) {
                foreach ($line in $commands) {
                    $fields = $line -split '\s{2,}'
                    if ($fields[0].Trim() -eq $resolvedAction -and $fields.Length -gt 3 -and $fields[3].Trim() -eq 'AGENT') {
                        $isAgent = $true
                        break
                    }
                }
            }

            if ($isAgent) {
                $script:ForgeActiveAgent = $resolvedAction
                $env:_FORGE_ACTIVE_AGENT = $resolvedAction
                if ($InputText) {
                    Invoke-ForgeChat $InputText
                } else {
                    Write-ForgeLog 'info' "$($resolvedAction.ToUpper()) is now the active agent"
                }
            } else {
                Write-ForgeLog 'error' "Unknown command: :$Action"
            }
        }
    }
}
