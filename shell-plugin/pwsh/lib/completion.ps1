# Forge PowerShell Plugin - Tab Completion
# Provides argument completion for the forge CLI.

Register-ArgumentCompleter -Native -CommandName forge -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandLine = $commandAst.ToString()

    # Use forge's built-in completion mechanism
    $env:_CLAP_COMPLETE = "powershell"
    $completions = & $script:ForgeBin complete -- "$commandLine" 2>$null
    Remove-Item Env:_CLAP_COMPLETE -ErrorAction SilentlyContinue

    if ($completions) {
        $completions | ForEach-Object {
            $parts = $_ -split '\t', 2
            $text = $parts[0]
            $tooltip = if ($parts.Length -gt 1) { $parts[1] } else { $text }
            [System.Management.Automation.CompletionResult]::new(
                $text, $text, 'ParameterValue', $tooltip
            )
        }
    }
}
