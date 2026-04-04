# Forge PowerShell Plugin - Session Configuration
# Session state variables for forge shell integration.

if (-not (Get-Variable -Name 'ForgeBin' -Scope Script -ErrorAction SilentlyContinue)) {
    $script:ForgeBin = if ($env:FORGE_BIN) { $env:FORGE_BIN } else { "forge" }
}

$script:ForgeConversationId = $null
$script:ForgePreviousConversationId = $null
$script:ForgeActiveAgent = $null
$script:ForgeSessionModel = $null
$script:ForgeSessionProvider = $null
$script:ForgeSessionReasoningEffort = $null
$script:ForgeMaxCommitDiff = if ($env:FORGE_MAX_COMMIT_DIFF) { [int]$env:FORGE_MAX_COMMIT_DIFF } else { 100000 }
$script:ForgeCommands = $null

# Tool detection
$script:ForgeFdCmd = if (Get-Command fdfind -ErrorAction SilentlyContinue) { 'fdfind' }
    elseif (Get-Command fd -ErrorAction SilentlyContinue) { 'fd' }
    else { $null }

$script:ForgeCatCmd = if (Get-Command bat -ErrorAction SilentlyContinue) {
    'bat --color=always --style=numbers,changes --line-range=:500'
} else { 'Get-Content' }

# Default nerd fonts to off on Windows PowerShell 5.1 (most terminals lack PUA glyphs)
# User can override with $env:NERD_FONT = "1"
if (-not $env:NERD_FONT -and -not $env:USE_NERD_FONT) {
    if ($PSVersionTable.PSVersion.Major -le 5) {
        $env:NERD_FONT = "0"
    }
}
