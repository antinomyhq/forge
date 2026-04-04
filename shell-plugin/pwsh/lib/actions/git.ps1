# Forge PowerShell Plugin - Git Actions

function Invoke-ForgeActionCommit {
    param([string]$InputText)

    $args = @('commit')
    if ($InputText) {
        $args += $InputText
    }
    Invoke-ForgeExec @args
}

function Invoke-ForgeActionCommitPreview {
    param([string]$InputText)

    $args = @('commit', '--preview')
    if ($InputText) {
        $args += $InputText
    }

    $result = Invoke-ForgeExec @args 2>$null
    if ($result) {
        # Check for staged changes
        $staged = git diff --cached --quiet 2>$null
        $prefix = if ($LASTEXITCODE -ne 0) { 'git commit -m' } else { 'git commit -am' }
        $message = $result -join "`n"
        Write-Host "$prefix `"$message`""
    }
}

function Invoke-ForgeActionSuggest {
    param([string]$InputText)

    if (-not $InputText) {
        Write-ForgeLog 'error' 'Usage: :suggest <description of what you want to do>'
        return
    }

    $result = & $script:ForgeBin suggest $InputText 2>$null
    if ($result) {
        Write-Host $result
    }
}
