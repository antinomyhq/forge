# Forge PowerShell Plugin - Config Actions

function Invoke-ForgeActionAgent {
    param([string]$InputText)

    if ($InputText) {
        $script:ForgeActiveAgent = $InputText
        $env:_FORGE_ACTIVE_AGENT = $InputText
        Write-ForgeLog 'success' "Switched to agent: $InputText"
        return
    }

    $agents = & $script:ForgeBin list agents --porcelain 2>$null
    if (-not $agents) {
        Write-ForgeLog 'error' 'Failed to list agents'
        return
    }

    if (Get-Command fzf -ErrorAction SilentlyContinue) {
        $selected = $agents |
            Where-Object { $_ -match '\S' } |
            Select-Object -Skip 1 |
            Invoke-ForgeFzf -FzfArgs @("--prompt=Agent > ")

        if ($selected) {
            $agentId = ($selected -split '\s{2,}')[0].Trim()
            $script:ForgeActiveAgent = $agentId
            $env:_FORGE_ACTIVE_AGENT = $agentId
            Write-ForgeLog 'success' "Switched to agent: $agentId"
        }
    } else {
        $agents | Write-Host
    }
}

function Invoke-ForgeActionSessionModel {
    param([string]$InputText)

    if ($InputText) {
        $script:ForgeSessionModel = $InputText
        $env:FORGE_SESSION__MODEL_ID = $InputText
        Write-ForgeLog 'success' "Session model: $InputText"
        return
    }

    $models = & $script:ForgeBin list models --porcelain 2>$null
    if (-not $models -or (Get-Command fzf -ErrorAction SilentlyContinue) -eq $null) {
        Write-ForgeLog 'error' 'Requires fzf for interactive selection, or provide model name as argument'
        return
    }

    $selected = $models |
        Where-Object { $_ -match '\S' } |
        Select-Object -Skip 1 |
        Invoke-ForgeFzf -FzfArgs @("--prompt=Model > ")

    if ($selected) {
        $modelId = ($selected -split '\s{2,}')[0].Trim()
        $script:ForgeSessionModel = $modelId
        $env:FORGE_SESSION__MODEL_ID = $modelId
        Write-ForgeLog 'success' "Session model: $modelId"
    }
}

function Invoke-ForgeActionConfigModel {
    param([string]$InputText)
    if ($InputText) {
        Invoke-ForgeExec config set model $InputText
    } else {
        Invoke-ForgeExec config set model
    }
}

function Invoke-ForgeActionCommitModel {
    param([string]$InputText)
    if ($InputText) {
        Invoke-ForgeExec config set commit-model $InputText
    } else {
        Invoke-ForgeExec config set commit-model
    }
}

function Invoke-ForgeActionSuggestModel {
    param([string]$InputText)
    if ($InputText) {
        Invoke-ForgeExec config set suggest-model $InputText
    } else {
        Invoke-ForgeExec config set suggest-model
    }
}

function Invoke-ForgeActionReasoningEffort {
    param([string]$InputText)

    if ($InputText) {
        $script:ForgeSessionReasoningEffort = $InputText
        $env:FORGE_REASONING__EFFORT = $InputText
        Write-ForgeLog 'success' "Session reasoning effort: $InputText"
        return
    }

    $choices = @('low', 'medium', 'high')
    if (Get-Command fzf -ErrorAction SilentlyContinue) {
        $selected = $choices | Invoke-ForgeFzf -FzfArgs @("--prompt=Reasoning Effort > ")
        if ($selected) {
            $script:ForgeSessionReasoningEffort = $selected
            $env:FORGE_REASONING__EFFORT = $selected
            Write-ForgeLog 'success' "Session reasoning effort: $selected"
        }
    } else {
        Write-ForgeLog 'info' "Options: $($choices -join ', '). Use :reasoning-effort <level>"
    }
}

function Invoke-ForgeActionConfigReasoningEffort {
    param([string]$InputText)
    if ($InputText) {
        Invoke-ForgeExec config set reasoning-effort $InputText
    } else {
        Invoke-ForgeExec config set reasoning-effort
    }
}

function Invoke-ForgeActionConfig {
    Invoke-ForgeExec config list
}

function Invoke-ForgeActionConfigEdit {
    $configPath = Join-Path $HOME "forge" ".forge.toml"
    $editor = if ($env:FORGE_EDITOR) { $env:FORGE_EDITOR } elseif ($env:EDITOR) { $env:EDITOR } else { "notepad" }
    & $editor $configPath
}

function Invoke-ForgeActionConfigReload {
    $script:ForgeSessionModel = $null
    $script:ForgeSessionProvider = $null
    $script:ForgeSessionReasoningEffort = $null
    Remove-Item Env:FORGE_SESSION__MODEL_ID -ErrorAction SilentlyContinue
    Remove-Item Env:FORGE_SESSION__PROVIDER_ID -ErrorAction SilentlyContinue
    Remove-Item Env:FORGE_REASONING__EFFORT -ErrorAction SilentlyContinue
    Write-ForgeLog 'success' 'Session overrides cleared'
}

function Invoke-ForgeActionTools {
    Invoke-ForgeExec list tools
}

function Invoke-ForgeActionSkills {
    Invoke-ForgeExec list skills
}

function Invoke-ForgeActionSync {
    Write-ForgeLog 'info' 'Syncing workspace...'
    Invoke-ForgeExec workspace sync --init
}
