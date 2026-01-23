#!/usr/bin/env zsh

# ZSH Keyboard Shortcuts - Display ZLE keyboard shortcuts
# Shows platform-specific keyboard shortcuts for ZSH Line Editor

# ANSI codes
RESET='\033[0m'
BOLD='\033[1m'
DIM='\033[2m'
CYAN='\033[0;36m'

# Text formatting helpers - auto-reset
function bold() { echo "${BOLD}${1}${RESET}"; }
function dim() { echo "${DIM}${1}${RESET}"; }
function cyan() { echo "${CYAN}${1}${RESET}"; }

# Helper function to print section headers
function print_section() {
    echo ""
    echo "$(bold "$1")"
}

# Helper function to print shortcuts with automatic padding
# Usage: print_shortcut "key" "description"
# If only one argument, prints it as-is (for special messages)
function print_shortcut() {
    local key=$1
    local description=$2

    if [[ -z "$description" ]]; then
        # Single argument - print as-is (for configuration lines)
        echo "  $(dim "${key}")"
    else
        # Two arguments - pad the key and align descriptions
        # Calculate padding based on key length (max 20 chars for alignment)
        local padding=20
        local key_len=${#key}
        local pad_len=$((padding - key_len))
        local pad=""
        if [[ $pad_len -gt 0 ]]; then
            printf -v pad "%${pad_len}s" ""
        fi
        printf "  $(cyan "%s")%s%s\n" "$key" "$pad" "$description"
    fi
}

# Detect platform
platform="unknown"
alt_key="Alt"
if [[ "$(uname)" == "Darwin" ]]; then
    platform="macOS"
    alt_key="Option"
elif [[ "$(uname)" == "Linux" ]]; then
    platform="Linux"
elif [[ "$(uname)" =~ "MINGW" ]] || [[ "$(uname)" =~ "MSYS" ]] || [[ "$(uname)" =~ "CYGWIN" ]] || [[ -n "$WINDIR" ]]; then
    platform="Windows"
fi

# Detect if vi/vim mode is enabled
vi_mode=false
# Check if main keymap is bound to viins or vicmd
if bindkey -lL main 2>/dev/null | grep -q "bindkey -A viins main\|bindkey -A vicmd main"; then
    vi_mode=true
fi

# Show platform and mode info
print_section "Configuration"
if [[ "$platform" != "unknown" ]]; then
    print_shortcut "Platform: ${platform}"
fi
if [[ "$vi_mode" == "true" ]]; then
    print_shortcut "Mode: Vi/Vim keybindings"
else
    print_shortcut "Mode: Emacs keybindings (default)"
fi

if [[ "$vi_mode" == "true" ]]; then
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
    print_shortcut "k / ↑" "Previous command"
    print_shortcut "j / ↓" "Next command"
    print_shortcut "/" "Search history backward"
    print_shortcut "?" "Search history forward"
    print_shortcut "n" "Next search match"
    print_shortcut "N" "Previous search match"
    
    print_section "Insert Mode"
    print_shortcut "Ctrl+W" "Delete word before cursor"
    print_shortcut "Ctrl+U" "Delete from cursor to start"
    
    print_section "Other"
    print_shortcut "v" "Edit command in \$EDITOR"
    print_shortcut "Ctrl+L" "Clear screen"
    print_shortcut "Ctrl+C" "Cancel current command"
    print_shortcut "Ctrl+Z" "Suspend current command"
    print_shortcut "Tab" "Complete command/path"
else
    # Emacs mode shortcuts (default)
    print_section "Line Navigation"
    print_shortcut "Ctrl+A" "Move to beginning of line"
    print_shortcut "Ctrl+E" "Move to end of line"        
    print_shortcut "${alt_key}+F" "Move forward one word"
    print_shortcut "${alt_key}+B" "Move backward one word"
    
    print_section "Editing"
    print_shortcut "Ctrl+U" "Kill line before cursor"
    print_shortcut "Ctrl+K" "Kill line after cursor"
    print_shortcut "Ctrl+W" "Kill word before cursor"
    print_shortcut "${alt_key}+D" "Kill word after cursor"
    print_shortcut "Ctrl+Y" "Yank (paste) killed text"
    print_shortcut "Ctrl+_" "Undo last edit"            
    
    print_section "History"
    print_shortcut "Ctrl+R" "Search command history backward"
    print_shortcut "Ctrl+S" "Search command history forward"
    print_shortcut "Ctrl+P / ↑" "Previous command"
    print_shortcut "Ctrl+N / ↓" "Next command"
    print_shortcut "${alt_key}+<" "Move to first history entry"
    print_shortcut "${alt_key}+>" "Move to last history entry"
    
    print_section "Other"
    print_shortcut "Ctrl+L" "Clear screen"
    print_shortcut "Ctrl+C" "Cancel current command"
    print_shortcut "Ctrl+Z" "Suspend current command"
    print_shortcut "Tab" "Complete command/path"
    
    echo ""
    if [[ "$platform" == "macOS" ]]; then
        echo "  $(dim "If Option key shortcuts don't work, run: forge zsh doctor")"
    elif [[ "$platform" == "Linux" ]]; then
        echo "  $(dim "If Alt key shortcuts don't work, run: forge zsh doctor")"
    fi
    echo "  $(dim "To enable Vi mode, add to ~/.zshrc: bindkey -v")"
fi

echo ""
