#!/usr/bin/env fish

# Fish Doctor - Diagnostic tool for Forge shell environment
# Port of shell-plugin/doctor.zsh adapted for fish shell.
# Checks for common configuration issues and environment setup.

# Source user's config.fish to get their environment (suppress errors)
if test -f "$HOME/.config/fish/config.fish"
    source "$HOME/.config/fish/config.fish" 2>/dev/null
end

# ANSI codes
set -l RESET '\033[0m'
set -l _BOLD '\033[1m'
set -l _DIM '\033[2m'
set -l _GREEN '\033[0;32m'
set -l _RED '\033[0;31m'
set -l _YELLOW '\033[0;33m'
set -l _CYAN '\033[0;36m'

# Text formatting helpers
function _bold
    printf '%b%s%b' "$_BOLD" "$argv[1]" "$RESET"
end
function _dim
    printf '%b%s%b' "$_DIM" "$argv[1]" "$RESET"
end
function _green
    printf '%b%s%b' "$_GREEN" "$argv[1]" "$RESET"
end
function _red
    printf '%b%s%b' "$_RED" "$argv[1]" "$RESET"
end
function _yellow
    printf '%b%s%b' "$_YELLOW" "$argv[1]" "$RESET"
end
function _cyan
    printf '%b%s%b' "$_CYAN" "$argv[1]" "$RESET"
end

# Simple ASCII symbols
set -l PASS "[OK]"
set -l FAIL "[ERROR]"
set -l WARN "[WARN]"

# Counters
set -g passed 0
set -g failed 0
set -g warnings 0

# Helper function to print section headers
function print_section
    echo ""
    printf '%b%s%b\n' "$_BOLD" "$argv[1]" "$RESET"
end

# Helper function to print results
function print_result
    set -l result_status $argv[1]
    set -l message $argv[2]
    set -l detail ""
    if test (count $argv) -ge 3
        set detail $argv[3]
    end

    switch $result_status
        case pass
            printf '  %b%s%b %s\n' "$_GREEN" "$PASS" "$RESET" "$message"
            set -g passed (math $passed + 1)
        case fail
            printf '  %b%s%b %s\n' "$_RED" "$FAIL" "$RESET" "$message"
            if test -n "$detail"
                printf '  %b%s%b\n' "$_DIM" "- $detail" "$RESET"
            end
            set -g failed (math $failed + 1)
        case warn
            printf '  %b%s%b %s\n' "$_YELLOW" "$WARN" "$RESET" "$message"
            if test -n "$detail"
                printf '  %b%s%b\n' "$_DIM" "- $detail" "$RESET"
            end
            set -g warnings (math $warnings + 1)
        case info
            printf '  %b%s%b\n' "$_DIM" "- $message" "$RESET"
        case code
            printf '  %b%s%b\n' "$_DIM" "- $message" "$RESET"
        case instruction
            printf '  %b%s%b\n' "$_DIM" "- $message" "$RESET"
    end
end

# Helper function to compare versions
# Returns 0 if version1 >= version2, 1 otherwise
function version_gte
    set -l version1 (string replace -r '^v' '' -- $argv[1])
    set -l version2 (string replace -r '^v' '' -- $argv[2])

    set -l ver1_parts (string split '.' -- "$version1")
    set -l ver2_parts (string split '.' -- "$version2")

    for i in 1 2 3
        set -l v1 0
        set -l v2 0
        if test (count $ver1_parts) -ge $i
            # Remove any non-numeric suffix (e.g., "0-rc1" -> "0")
            set v1 (string replace -r '[^0-9].*' '' -- "$ver1_parts[$i]")
            if test -z "$v1"
                set v1 0
            end
        end
        if test (count $ver2_parts) -ge $i
            set v2 (string replace -r '[^0-9].*' '' -- "$ver2_parts[$i]")
            if test -z "$v2"
                set v2 0
            end
        end

        if test $v1 -gt $v2
            return 0
        else if test $v1 -lt $v2
            return 1
        end
    end

    return 0 # versions are equal
end

printf '%b%s%b\n' "$_BOLD" "FORGE ENVIRONMENT DIAGNOSTICS" "$RESET"

# 1. Check fish version
print_section "Shell Environment"
set -l fish_ver "$FISH_VERSION"
if test -n "$fish_ver"
    set -l major (echo $fish_ver | cut -d. -f1)
    set -l minor (echo $fish_ver | cut -d. -f2)
    if test $major -ge 3; and test $minor -ge 3; or test $major -gt 3
        print_result pass "fish: $fish_ver"
    else
        print_result warn "fish: $fish_ver" "Recommended: 3.3+"
    end
else
    print_result fail "Unable to detect fish version"
end

# Check terminal information
if test -n "$TERM_PROGRAM"
    if test -n "$TERM_PROGRAM_VERSION"
        print_result pass "Terminal: $TERM_PROGRAM $TERM_PROGRAM_VERSION"
    else
        print_result pass "Terminal: $TERM_PROGRAM"
    end
else if test -n "$TERM"
    print_result pass "Terminal: $TERM"
else
    print_result info "Terminal: unknown"
end

# 2. Check if forge is installed and in PATH
print_section "Forge Installation"

if command -q forge
    set -l forge_path (command -v forge)

    # Get forge version and extract just the version number
    set -l forge_version (forge --version 2>&1 | head -n1 | awk '{print $2}')
    if test -n "$forge_version"
        print_result pass "forge: $forge_version"
        print_result info "$forge_path"
    else
        print_result pass "forge: installed"
        print_result info "$forge_path"
    end
else
    print_result fail "Forge binary not found in PATH" "Installation: curl -fsSL https://forgecode.dev/cli | sh"
end

# 3. Check shell plugin
print_section "Plugin"

# Check if forge plugin is loaded by checking environment variable
if test -n "$_FORGE_PLUGIN_LOADED"
    print_result pass "Forge plugin loaded"
else
    print_result fail "Forge plugin not loaded"
    print_result instruction "Add to your ~/.config/fish/config.fish:"
    print_result code "forge fish plugin | source"
    print_result instruction "Or run: forge fish setup"
end

# Check plugin loading in config.fish
set -l config_fish "$HOME/.config/fish/config.fish"
if test -f "$config_fish"; and test -n "$_FORGE_PLUGIN_LOADED"
    # Check if the forge plugin line exists
    if string match -rq 'forge.*fish.*plugin.*source' < "$config_fish" 2>/dev/null
        print_result pass "Plugin configured in config.fish"
    end
end

# 4. Check theme/prompt
print_section "FORGE RIGHT PROMPT"

if test -n "$_FORGE_THEME_LOADED"
    print_result pass "Forge theme loaded"
else
    # Check if user has a custom fish_right_prompt
    if functions -q fish_right_prompt
        print_result warn "Custom right prompt detected"
        print_result instruction "To use Forge theme, add to ~/.config/fish/config.fish:"
        print_result code "forge fish theme | source"
    else
        print_result warn "No theme loaded"
        print_result instruction "To use Forge theme, add to ~/.config/fish/config.fish:"
        print_result code "forge fish theme | source"
    end
end

# 5. Check dependencies
print_section "Dependencies"

# Check for fzf - required for interactive selection
if command -q fzf
    set -l fzf_version (fzf --version 2>&1 | head -n1 | awk '{print $1}')
    if test -n "$fzf_version"
        if version_gte "$fzf_version" "0.36.0"
            print_result pass "fzf: $fzf_version"
        else
            print_result fail "fzf: $fzf_version" "Version 0.36.0 or higher required. Update: https://github.com/junegunn/fzf#installation"
        end
    else
        print_result pass "fzf: installed"
    end
else
    print_result fail "fzf not found" "Required for interactive features. See installation: https://github.com/junegunn/fzf#installation"
end

# Check for fd/fdfind - used for file discovery
if command -q fd
    set -l fd_version (fd --version 2>&1 | awk '{print $2}')
    if test -n "$fd_version"
        if version_gte "$fd_version" "10.0.0"
            print_result pass "fd: $fd_version"
        else
            print_result fail "fd: $fd_version" "Version 10.0.0 or higher required. Update: https://github.com/sharkdp/fd#installation"
        end
    else
        print_result pass "fd: installed"
    end
else if command -q fdfind
    set -l fd_version (fdfind --version 2>&1 | awk '{print $2}')
    if test -n "$fd_version"
        if version_gte "$fd_version" "10.0.0"
            print_result pass "fdfind: $fd_version"
        else
            print_result fail "fdfind: $fd_version" "Version 10.0.0 or higher required. Update: https://github.com/sharkdp/fd#installation"
        end
    else
        print_result pass "fdfind: installed"
    end
else
    print_result warn "fd/fdfind not found" "Enhanced file discovery. See installation: https://github.com/sharkdp/fd#installation"
end

# Check for bat - used for syntax highlighting
if command -q bat
    set -l bat_version (bat --version 2>&1 | awk '{print $2}')
    if test -n "$bat_version"
        if version_gte "$bat_version" "0.20.0"
            print_result pass "bat: $bat_version"
        else
            print_result fail "bat: $bat_version" "Version 0.20.0 or higher required. Update: https://github.com/sharkdp/bat#installation"
        end
    else
        print_result pass "bat: installed"
    end
else
    print_result warn "bat not found" "Enhanced preview. See installation: https://github.com/sharkdp/bat#installation"
end

# 6. Check system configuration
print_section "System"

# Check editor configuration (FORGE_EDITOR takes precedence over EDITOR)
if test -n "$FORGE_EDITOR"
    print_result pass "FORGE_EDITOR: $FORGE_EDITOR"
    if test -n "$EDITOR"
        print_result info "EDITOR also set: $EDITOR (ignored)"
    end
else if test -n "$EDITOR"
    print_result pass "EDITOR: $EDITOR"
    print_result info "TIP: Set FORGE_EDITOR for forge-specific editor"
else
    print_result warn "No editor configured" "set -Ux EDITOR vim or set -Ux FORGE_EDITOR vim"
end

# Check PATH for common issues
if string match -q '*/usr/local/bin*' -- "$PATH"; or string match -q '*/usr/bin*' -- "$PATH"
    print_result pass "PATH: configured"
else
    print_result warn "PATH may need common directories" "Ensure /usr/local/bin or /usr/bin is in PATH"
end

# 7. Check keyboard configuration (Alt/Option key as Meta)
print_section "Keyboard Configuration"

set -l platform (uname)
set -l meta_key_ok false
set -l check_performed false

if test "$platform" = Darwin
    # macOS checks
    if test "$TERM_PROGRAM" = vscode
        set check_performed true
        set -l vscode_settings "$HOME/Library/Application Support/Code/User/settings.json"
        if test -f "$vscode_settings"
            if string match -rq '"terminal.integrated.macOptionIsMeta"\s*:\s*true' < "$vscode_settings" 2>/dev/null
                print_result pass "VS Code: Option key configured as Meta"
                set meta_key_ok true
            else
                print_result warn "VS Code: Option key NOT configured as Meta"
                print_result instruction "Option+F and Option+B shortcuts won't work for word navigation"
                print_result instruction "Add to VS Code settings.json:"
                print_result code '"terminal.integrated.macOptionIsMeta": true'
                print_result instruction "Then reload VS Code: Cmd+Shift+P -> Reload Window"
            end
        else
            print_result warn "VS Code settings file not found"
            print_result info "Expected: $vscode_settings"
        end
    else if test "$TERM_PROGRAM" = iTerm.app
        set check_performed true
        set -l iterm_prefs "$HOME/Library/Preferences/com.googlecode.iterm2.plist"
        if test -f "$iterm_prefs"
            set -l option_setting (defaults read com.googlecode.iterm2 2>/dev/null | string match -r '"(Left |Right )?Option Key Sends"\s*=\s*([0-9])' | string match -r '[0-9]' | head -1)
            if test "$option_setting" = 2
                print_result pass "iTerm2: Option key configured as Esc+"
                set meta_key_ok true
            else
                print_result warn "iTerm2: Option key NOT configured as Esc+"
                print_result instruction "Option+F and Option+B shortcuts won't work for word navigation"
                print_result instruction "Configure in iTerm2:"
                print_result info "Preferences -> Profiles -> Keys -> Left/Right Option Key -> Esc+"
            end
        else
            print_result warn "iTerm2 preferences not found"
            print_result info "Expected: $iterm_prefs"
        end
    else if test "$TERM_PROGRAM" = Apple_Terminal
        set check_performed true
        set -l terminal_prefs "$HOME/Library/Preferences/com.apple.Terminal.plist"
        if test -f "$terminal_prefs"
            set -l use_option (defaults read com.apple.Terminal 2>/dev/null | string match -r 'useOptionAsMetaKey\s*=\s*([0-9])' | string match -r '[0-9]' | head -1)
            if test "$use_option" = 1
                print_result pass "Terminal.app: Option key configured as Meta"
                set meta_key_ok true
            else
                print_result warn "Terminal.app: Option key NOT configured as Meta"
                print_result instruction "Option+F and Option+B shortcuts won't work for word navigation"
                print_result instruction "Configure in Terminal.app:"
                print_result info "Preferences -> Profiles -> Keyboard -> Use Option as Meta key"
            end
        else
            print_result warn "Terminal.app preferences not found"
            print_result info "Expected: $terminal_prefs"
        end
    end

    # If no specific terminal detected, provide general guidance for macOS
    if test "$check_performed" = false
        print_result info "Terminal: $TERM_PROGRAM"
        print_result info "For Option key shortcuts (word navigation) to work:"
        print_result info "- VS Code: Settings -> terminal.integrated.macOptionIsMeta -> true"
        print_result info "- iTerm2: Preferences -> Profiles -> Keys -> Option Key -> Esc+"
        print_result info "- Terminal.app: Preferences -> Profiles -> Keyboard -> Use Option as Meta"
        print_result info "Run 'forge fish keyboard' for detailed keyboard shortcuts"
    end

else if test "$platform" = Linux
    # Linux checks
    if test "$TERM_PROGRAM" = vscode
        set check_performed true
        set -l vscode_settings "$HOME/.config/Code/User/settings.json"
        if test -f "$vscode_settings"
            if string match -rq '"terminal.integrated.sendAltAsMetaKey"\s*:\s*true' < "$vscode_settings" 2>/dev/null
                or string match -rq '"terminal.integrated.macOptionIsMeta"\s*:\s*true' < "$vscode_settings" 2>/dev/null
                print_result pass "VS Code: Alt key configured as Meta"
                set meta_key_ok true
            else
                print_result warn "VS Code: Alt key NOT configured as Meta"
                print_result instruction "Alt+F and Alt+B shortcuts won't work for word navigation"
                print_result instruction "Add to VS Code settings.json:"
                print_result code '"terminal.integrated.sendAltAsMetaKey": true'
                print_result instruction "Then reload VS Code: Ctrl+Shift+P -> Reload Window"
            end
        else
            print_result warn "VS Code settings file not found"
            print_result info "Expected: $vscode_settings"
        end
    else if test -n "$GNOME_TERMINAL_SERVICE"; or test "$COLORTERM" = gnome-terminal
        set check_performed true
        print_result pass "GNOME Terminal: Alt key typically works by default"
        print_result info "If Alt+F/B don't work, check: Preferences -> Profile -> Keyboard"
        set meta_key_ok true
    else if test "$COLORTERM" = truecolor; and command -q konsole
        set check_performed true
        print_result pass "Konsole: Alt key typically works by default"
        print_result info "If Alt+F/B don't work, check: Settings -> Edit Profile -> Keyboard"
        set meta_key_ok true
    else if test -n "$ALACRITTY_SOCKET"; or test "$TERM" = alacritty
        set check_performed true
        print_result pass "Alacritty: Alt key typically works by default"
        print_result info "If Alt+F/B don't work, ensure no conflicting key bindings"
        set meta_key_ok true
    else if test "$TERM" = xterm; or test "$TERM" = xterm-256color
        set check_performed true
        set -l xresources "$HOME/.Xresources"
        if test -f "$xresources"
            if string match -rq 'XTerm\*metaSendsEscape:\s*true' < "$xresources" 2>/dev/null
                or string match -rq 'XTerm\*eightBitInput:\s*false' < "$xresources" 2>/dev/null
                print_result pass "xterm: Meta key configured"
                set meta_key_ok true
            else
                print_result warn "xterm: Meta key may not be configured"
                print_result instruction "Add to ~/.Xresources:"
                print_result code "XTerm*metaSendsEscape: true"
                print_result instruction "Then reload: xrdb ~/.Xresources"
            end
        else
            print_result info "xterm detected"
            print_result info "To enable Alt as Meta, add to ~/.Xresources:"
            print_result info "XTerm*metaSendsEscape: true"
        end
    end

    # If no specific terminal detected, provide general guidance for Linux
    if test "$check_performed" = false
        set -l term_info "$TERM_PROGRAM"
        if test -z "$term_info"
            set term_info "$TERM"
        end
        print_result info "Terminal: $term_info"
        print_result info "For Alt key shortcuts (word navigation) to work:"
        print_result info "- VS Code: Settings -> terminal.integrated.sendAltAsMetaKey -> true"
        print_result info "- GNOME Terminal: Usually works by default"
        print_result info "- Konsole: Usually works by default"
        print_result info "- xterm: Add 'XTerm*metaSendsEscape: true' to ~/.Xresources"
        print_result info "Run 'forge fish keyboard' for detailed keyboard shortcuts"
    end
else
    # Other platforms (BSD, etc.)
    print_result info "Keyboard check: Platform $platform - manual verification needed"
    print_result info "Ensure Alt/Meta key is configured for word navigation shortcuts"
end

# 8. Check font and Nerd Font support
print_section "Nerd Font"

# Check if Nerd Font is enabled via environment variables
if test -n "$NERD_FONT"
    if test "$NERD_FONT" = 1; or test "$NERD_FONT" = true
        print_result pass "NERD_FONT: enabled"
    else
        print_result warn "NERD_FONT: disabled ($NERD_FONT)"
        print_result instruction "Enable Nerd Font by setting:"
        print_result code "set -Ux NERD_FONT 1"
    end
else if test -n "$USE_NERD_FONT"
    if test "$USE_NERD_FONT" = 1; or test "$USE_NERD_FONT" = true
        print_result pass "USE_NERD_FONT: enabled"
    else
        print_result warn "USE_NERD_FONT: disabled ($USE_NERD_FONT)"
        print_result instruction "Enable Nerd Font by setting:"
        print_result code "set -Ux NERD_FONT 1"
    end
else
    print_result pass "Nerd Font: enabled (default)"
    print_result info "Forge will auto-detect based on terminal capabilities"
end

# Show actual icons used in Forge theme for manual verification (skip if explicitly disabled)
set -l nerd_font_disabled false
if test -n "$NERD_FONT"; and test "$NERD_FONT" != 1; and test "$NERD_FONT" != true
    set nerd_font_disabled true
else if test -n "$USE_NERD_FONT"; and test "$USE_NERD_FONT" != 1; and test "$USE_NERD_FONT" != true
    set nerd_font_disabled true
end

if test "$nerd_font_disabled" = false
    echo ""
    printf '  %b%s%b\n' "$_YELLOW" "Visual Check [Manual Verification Required]" "$RESET"
    printf '   %b%s%b %b%s%b\n' "$_BOLD" "󱙺 FORGE 33.0k" "$RESET" "$_CYAN" " tonic-1.0" "$RESET"
    echo ""
    echo "   Forge uses Nerd Fonts to enrich cli experience, can you see all the icons clearly without any overlap?"
    echo "   If you see boxes or question marks, install a Nerd Font from:"
    printf '   %b%s%b\n' "$_DIM" "https://www.nerdfonts.com/" "$RESET"
    echo ""
end

# Summary
echo ""

if test $failed -eq 0; and test $warnings -eq 0
    printf '%b%s%b %b%s%b %b%s%b\n' "$_GREEN" "$PASS" "$RESET" "$_BOLD" "All checks passed" "$RESET" "$_DIM" "($passed)" "$RESET"
    exit 0
else if test $failed -eq 0
    printf '%b%s%b %b%s%b %b%s%b\n' "$_YELLOW" "$WARN" "$RESET" "$_BOLD" "$warnings warnings" "$RESET" "$_DIM" "($passed passed)" "$RESET"
    exit 0
else
    printf '%b%s%b %b%s%b %b%s%b\n' "$_RED" "$FAIL" "$RESET" "$_BOLD" "$failed failed" "$RESET" "$_DIM" "($warnings warnings, $passed passed)" "$RESET"
    exit 1
end
