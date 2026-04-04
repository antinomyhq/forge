# Forge PowerShell Plugin - Command Interception
# Uses a PreCommandLookupAction to intercept :command patterns before
# PowerShell tries to execute them. Works on all PowerShell versions.

$ExecutionContext.SessionState.InvokeCommand.PreCommandLookupAction = {
    param($commandName, $commandLookupEventArgs)

    # Check if the command starts with ':'
    if ($commandName -match '^:(.*)$') {
        # Mark as handled so PowerShell doesn't throw "command not found"
        $commandLookupEventArgs.StopSearch = $true
        $commandLookupEventArgs.CommandScriptBlock = {
            # Parse the full invocation line from $MyInvocation
            $fullLine = $MyInvocation.Line.Trim()

            if ($fullLine -match '^:([a-zA-Z][a-zA-Z0-9_-]*)(\s+(.*))?$') {
                $action = $Matches[1]
                $text = if ($Matches[3]) { $Matches[3].Trim() } else { "" }
                Invoke-ForgeDispatch -Action $action -InputText $text
            }
            elseif ($fullLine -match '^:\s+(.+)$') {
                $text = $Matches[1].Trim()
                Invoke-ForgeDispatch -Action "" -InputText $text
            }
            else {
                # Just ":" with nothing — ignore
            }
        }.GetNewClosure()
    }
}
