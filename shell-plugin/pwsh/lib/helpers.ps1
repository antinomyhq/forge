# Forge PowerShell Plugin - Helper Functions

function Get-ForgeCommands {
    if (-not $script:ForgeCommands) {
        $origColor = $env:CLICOLOR_FORCE
        $env:CLICOLOR_FORCE = "0"
        $script:ForgeCommands = & $script:ForgeBin list commands --porcelain 2>$null
        $env:CLICOLOR_FORCE = $origColor
    }
    return $script:ForgeCommands
}

function Invoke-ForgeExec {
    param(
        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]]$Arguments
    )

    $agentId = if ($script:ForgeActiveAgent) { $script:ForgeActiveAgent } else { "forge" }
    $cmd = @("--agent", $agentId)

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
    if ($script:ForgeSessionReasoningEffort) {
        $env:FORGE_REASONING__EFFORT = $script:ForgeSessionReasoningEffort
    }

    & $script:ForgeBin @cmd @Arguments
}

function Invoke-ForgeFzf {
    param(
        [Parameter(ValueFromPipeline = $true)]
        [string[]]$InputObject,
        [string[]]$FzfArgs = @()
    )

    begin { $lines = @() }
    process { $lines += $InputObject }
    end {
        $defaultArgs = @(
            '--reverse', '--exact', '--cycle', '--select-1',
            '--height', '80%', '--no-scrollbar', '--ansi',
            '--color=header:bold'
        )
        $allArgs = $defaultArgs + $FzfArgs
        $lines | & fzf @allArgs
    }
}

function Write-ForgeLog {
    param(
        [ValidateSet('error', 'info', 'success', 'warning', 'debug')]
        [string]$Level,
        [string]$Message
    )

    $ts = Get-Date -Format "HH:mm:ss"
    $e = [char]27

    switch ($Level) {
        'error'   { Write-Host "$e[31m*$e[0m [$ts] $e[31m$Message$e[0m" }
        'info'    { Write-Host "$e[37m*$e[0m [$ts] $e[37m$Message$e[0m" }
        'success' { Write-Host "$e[33m*$e[0m [$ts] $e[37m$Message$e[0m" }
        'warning' { Write-Host "$e[93m!$e[0m [$ts] $e[93m$Message$e[0m" }
        'debug'   { Write-Host "$e[36m*$e[0m [$ts] $e[90m$Message$e[0m" }
    }
}

function Switch-ForgeConversation {
    param([string]$NewId)
    if ($script:ForgeConversationId) {
        $script:ForgePreviousConversationId = $script:ForgeConversationId
    }
    $script:ForgeConversationId = $NewId
    $env:_FORGE_CONVERSATION_ID = $NewId
}

function Clear-ForgeConversation {
    if ($script:ForgeConversationId) {
        $script:ForgePreviousConversationId = $script:ForgeConversationId
    }
    $script:ForgeConversationId = $null
    $env:_FORGE_CONVERSATION_ID = $null
}

function Start-ForgeBackgroundSync {
    Start-Job -ScriptBlock {
        param($bin)
        & $bin workspace sync --init 2>$null
    } -ArgumentList $script:ForgeBin | Out-Null
}
