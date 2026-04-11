#!/usr/bin/env fish

# Fish Keyboard Shortcuts - Display fish key bindings
# Port of shell-plugin/keyboard.zsh adapted for fish shell.
# Shows platform-specific keyboard shortcuts for fish shell.

# ANSI codes
set RESET '\033[0m'
set BOLD '\033[1m'
set DIM '\033[2m'
set CYAN '\033[0;36m'

# Helper function to print section headers
function print_section
    echo ""
    printf '%b%s%b\n' "$BOLD" "$argv[1]" "$RESET"
end

# Helper function to print shortcuts with automatic padding
# Usage: print_shortcut "key" "description"
# If only one argument, prints it as-is (for special messages)
function print_shortcut
    set -l key $argv[1]
    set -l description ""
    if test (count $argv) -ge 2
        set description $argv[2]
    end

    if test -z "$description"
        # Single argument - print as-is (for configuration lines)
        printf '  %b%s%b\n' "$DIM" "$key" "$RESET"
    else
        # Two arguments - pad the key and align descriptions
        set -l padding 20
        set -l key_len (string length -- "$key")
        set -l pad_len (math $padding - $key_len)
        set -l pad ""
        if test $pad_len -gt 0
            set pad (string repeat -n $pad_len ' ')
        end
        printf '  %b%s%b%s%s\n' "$CYAN" "$key" "$RESET" "$pad" "$description"
    end
end

# Detect platform
set platform "unknown"
set alt_key "Alt"
switch (uname)
    case Darwin
        set platform "macOS"
        set alt_key "Option"
    case Linux
        set platform "Linux"
    case 'MINGW*' 'MSYS*' 'CYGWIN*'
        set platform "Windows"
end
if test "$platform" = unknown; and test -n "$WINDIR"
    set platform "Windows"
end

# Detect if vi mode is enabled
set vi_mode false
# Check if fish_key_bindings is set to fish_vi_key_bindings
if test "$fish_key_bindings" = fish_vi_key_bindings
    set vi_mode true
end

# Show platform and mode info
print_section "Configuration"
if test "$platform" != unknown
    print_shortcut "Platform: $platform"
end
if test "$vi_mode" = true
    print_shortcut "Mode: Vi/Vim keybindings"
else
    print_shortcut "Mode: Emacs keybindings (default)"
end

if test "$vi_mode" = true
    # Vim mode shortcuts
    print_section "Mode Switching"
    print_shortcut "ESC / Ctrl+[" "Enter command mode (normal)"
    print_shortcut "i" "Enter insert mode"
    print_shortcut "a" "Enter insert mode (after cursor)"
    print_shortcut "A" "Enter insert mode (end of line)"
    print_shortcut "I" "Enter insert mode (start of line)"

    print_section "Navigation (Command Mode)"
    print_shortcut "w" "Move forward one word"
    print_shortcut "b" "Move backward one word"
    print_shortcut "0 / ^" "Move to beginning of line"
    print_shortcut "\$" "Move to end of line"

    print_section "Editing (Command Mode)"
    print_shortcut "dd" "Delete entire line"
    print_shortcut "D" "Delete from cursor to end of line"
    print_shortcut "cc" "Change entire line"
    print_shortcut "C" "Change from cursor to end of line"
    print_shortcut "cw" "Change word"
    print_shortcut "dw" "Delete word"
    print_shortcut "u" "Undo"
    print_shortcut "p" "Paste after cursor"
    print_shortcut "P" "Paste before cursor"

    print_section "History (Command Mode)"
    print_shortcut "k / Up" "Previous command"
    print_shortcut "j / Down" "Next command"
    print_shortcut "/" "Search history backward"

    print_section "Insert Mode"
    print_shortcut "Ctrl+W" "Delete word before cursor"
    print_shortcut "Ctrl+U" "Delete from cursor to start"

    print_section "Fish-Specific"
    print_shortcut "Alt+Left" "Move backward one word"
    print_shortcut "Alt+Right" "Move forward one word"
    print_shortcut "Alt+Up" "Search history for token"
    print_shortcut "Alt+S" "Prepend 'sudo ' to command"

    print_section "Other"
    print_shortcut "Ctrl+L" "Clear screen"
    print_shortcut "Ctrl+C" "Cancel current command"
    print_shortcut "Ctrl+Z" "Suspend current command"
    print_shortcut "Tab" "Complete command/path"
    print_shortcut "Right" "Accept autosuggestion"
    print_shortcut "Alt+Right" "Accept one word of autosuggestion"
else
    # Emacs mode shortcuts (default)
    print_section "Line Navigation"
    print_shortcut "Ctrl+A" "Move to beginning of line"
    print_shortcut "Ctrl+E" "Move to end of line"
    print_shortcut "$alt_key+F" "Move forward one word"
    print_shortcut "$alt_key+B" "Move backward one word"
    print_shortcut "Alt+Left" "Move backward one word"
    print_shortcut "Alt+Right" "Move forward one word"

    print_section "Editing"
    print_shortcut "Ctrl+U" "Kill line before cursor"
    print_shortcut "Ctrl+K" "Kill line after cursor"
    print_shortcut "Ctrl+W" "Kill word before cursor"
    print_shortcut "$alt_key+D" "Kill word after cursor"
    print_shortcut "Ctrl+Y" "Yank (paste) killed text"
    print_shortcut "Ctrl+Z" "Undo last edit"

    print_section "History"
    print_shortcut "Ctrl+R" "Search command history backward"
    print_shortcut "Ctrl+P / Up" "Previous command"
    print_shortcut "Ctrl+N / Down" "Next command"
    print_shortcut "Alt+Up" "Search history for token"

    print_section "Autosuggestions"
    print_shortcut "Right" "Accept full autosuggestion"
    print_shortcut "Alt+Right" "Accept one word of autosuggestion"
    print_shortcut "Ctrl+F" "Accept full autosuggestion"

    print_section "Fish-Specific"
    print_shortcut "Alt+S" "Prepend 'sudo ' to command"
    print_shortcut "Alt+E" "Edit command in \$EDITOR"
    print_shortcut "Alt+P" "Page through output"
    print_shortcut "Alt+L" "List directory contents"

    print_section "Other"
    print_shortcut "Ctrl+L" "Clear screen"
    print_shortcut "Ctrl+C" "Cancel current command"
    print_shortcut "Ctrl+D" "Exit shell (on empty line)"
    print_shortcut "Tab" "Complete command/path"
    print_shortcut "Shift+Tab" "Cycle completion backward"

    echo ""
    if test "$platform" = macOS
        printf '  %b%s%b\n' "$DIM" "If Option key shortcuts don't work, run: forge fish doctor" "$RESET"
    else if test "$platform" = Linux
        printf '  %b%s%b\n' "$DIM" "If Alt key shortcuts don't work, run: forge fish doctor" "$RESET"
    end
    printf '  %b%s%b\n' "$DIM" "To enable Vi mode: set -U fish_key_bindings fish_vi_key_bindings" "$RESET"
end

echo ""
