# !! Contents within this block are managed by 'forge powershell setup' !!
# !! Do not edit manually - changes will be overwritten !!

# Load forge shell plugin if not already loaded
if (-not $env:_FORGE_PLUGIN_LOADED) {
    Invoke-Expression ((& forge powershell plugin) -join "`n")
}

# Load forge shell theme if not already loaded
if (-not $env:_FORGE_THEME_LOADED) {
    Invoke-Expression ((& forge powershell theme) -join "`n")
}
