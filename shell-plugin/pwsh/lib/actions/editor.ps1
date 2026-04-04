# Forge PowerShell Plugin - Editor Actions

function Invoke-ForgeActionEditor {
    $editor = if ($env:FORGE_EDITOR) { $env:FORGE_EDITOR }
        elseif ($env:EDITOR) { $env:EDITOR }
        else { "notepad" }

    $tempFile = [System.IO.Path]::GetTempFileName() + ".md"

    try {
        # Open editor with temp file
        $proc = Start-Process -FilePath $editor -ArgumentList $tempFile -Wait -PassThru -NoNewWindow

        if ($proc.ExitCode -eq 0 -and (Test-Path $tempFile)) {
            $content = Get-Content $tempFile -Raw
            if ($content -and $content.Trim()) {
                Invoke-ForgeExec $content.Trim()
            }
        }
    } finally {
        if (Test-Path $tempFile) {
            Remove-Item $tempFile -Force
        }
    }
}
