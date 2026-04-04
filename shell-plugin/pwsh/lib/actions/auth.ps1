# Forge PowerShell Plugin - Auth Actions

function Invoke-ForgeActionLogin {
    $providers = & $script:ForgeBin list provider --porcelain 2>$null
    if (-not $providers) {
        Write-ForgeLog 'error' 'Failed to list providers'
        return
    }

    if (Get-Command fzf -ErrorAction SilentlyContinue) {
        $selected = $providers |
            Where-Object { $_ -match '\S' } |
            Select-Object -Skip 1 |
            Invoke-ForgeFzf -FzfArgs @("--prompt=Login Provider > ")

        if ($selected) {
            $providerId = ($selected -split '\s{2,}')[0].Trim()
            Invoke-ForgeExec provider login $providerId
        }
    } else {
        $providers | Write-Host
        Write-ForgeLog 'info' 'Use: forge provider login <provider-id>'
    }
}

function Invoke-ForgeActionLogout {
    $providers = & $script:ForgeBin list provider --porcelain 2>$null
    if (-not $providers) {
        Write-ForgeLog 'error' 'Failed to list providers'
        return
    }

    if (Get-Command fzf -ErrorAction SilentlyContinue) {
        $selected = $providers |
            Where-Object { $_ -match '\S' } |
            Select-Object -Skip 1 |
            Where-Object { $_ -match 'logged.in|active' } |
            Invoke-ForgeFzf -FzfArgs @("--prompt=Logout Provider > ")

        if ($selected) {
            $providerId = ($selected -split '\s{2,}')[0].Trim()
            Invoke-ForgeExec provider logout $providerId
        }
    } else {
        $providers | Write-Host
        Write-ForgeLog 'info' 'Use: forge provider logout <provider-id>'
    }
}
