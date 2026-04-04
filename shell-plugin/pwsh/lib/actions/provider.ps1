# Forge PowerShell Plugin - Provider Actions

function Invoke-ForgeActionProvider {
    param([string]$InputText)

    if ($InputText) {
        Invoke-ForgeExec config set provider $InputText
        return
    }

    $providers = & $script:ForgeBin list provider --porcelain 2>$null
    if (-not $providers) {
        Write-ForgeLog 'error' 'Failed to list providers'
        return
    }

    if (Get-Command fzf -ErrorAction SilentlyContinue) {
        $selected = $providers |
            Where-Object { $_ -match '\S' } |
            Select-Object -Skip 1 |
            Invoke-ForgeFzf -FzfArgs @("--prompt=Provider > ")

        if ($selected) {
            $providerId = ($selected -split '\s{2,}')[0].Trim()
            Invoke-ForgeExec config set provider $providerId
        }
    } else {
        $providers | Write-Host
    }
}
