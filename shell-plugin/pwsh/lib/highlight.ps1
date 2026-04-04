# Forge PowerShell Plugin - Syntax Highlighting
# PSReadLine token-based coloring. Note: PSReadLine does not support
# regex-based pattern highlighting like zsh-syntax-highlighting,
# so :command highlighting is limited to token-level colors.

if (Get-Module PSReadLine) {
    $esc = [char]0x1b
    Set-PSReadLineOption -Colors @{
        Command   = "$esc[33m"   # Yellow for commands
        Parameter = "$esc[36m"   # Cyan for parameters
        String    = "$esc[32m"   # Green for strings
        Number    = "$esc[35m"   # Magenta for numbers
        Comment   = "$esc[90m"   # Gray for comments
    }
}
