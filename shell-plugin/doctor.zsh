#!/usr/bin/env zsh

# ZSH Doctor - Diagnostic tool for Forge shell environment
# Checks for common configuration issues and environment setup

# Source user's .zshrc to get their environment (suppress errors from non-interactive mode)
if [[ -f "${ZDOTDIR:-$HOME}/.zshrc" ]]; then
    source "${ZDOTDIR:-$HOME}/.zshrc" 2>/dev/null
fi

# ANSI codes
local RESET='\033[0m'
local _BOLD='\033[1m'
local _DIM='\033[2m'
local _GREEN='\033[0;32m'
local _RED='\033[0;31m'
local _YELLOW='\033[0;33m'
local _CYAN='\033[0;36m'

# Text formatting helpers - auto-reset
function bold() { echo "${_BOLD}${1}${RESET}"; }
function dim() { echo "${_DIM}${1}${RESET}"; }
function green() { echo "${_GREEN}${1}${RESET}"; }
function red() { echo "${_RED}${1}${RESET}"; }
function yellow() { echo "${_YELLOW}${1}${RESET}"; }
function cyan() { echo "${_CYAN}${1}${RESET}"; }

# Simple ASCII symbols
local PASS="[OK]"
local FAIL="[!!]"
local WARN="[--]"

# Counters
local passed=0
local failed=0
local warnings=0

# Helper function to print section headers
function print_section() {
    echo ""
    echo "$(bold "$1")"
}

# Helper function to print results
function print_result() {
    local result_status=$1
    local message=$2
    local detail=$3
    
    case $result_status in
        pass)
            echo "  $(green "${PASS}") ${message}"
            ((passed++))
            ;;
        fail)
            echo "  $(red "${FAIL}") ${message}"
            [[ -n "$detail" ]] && echo "       $(dim "${detail}")"
            ((failed++))
            ;;
        warn)
            echo "  $(yellow "${WARN}") ${message}"
            [[ -n "$detail" ]] && echo "       $(dim "${detail}")"
            ((warnings++))
            ;;
        info)
            echo "       $(dim "${message}")"
            ;;
    esac
}

echo "$(bold "FORGE ENVIRONMENT DIAGNOSTICS")"

# 1. Check ZSH version
print_section "Shell Environment"
local zsh_version="${ZSH_VERSION}"
if [[ -n "$zsh_version" ]]; then
    local major=$(echo $zsh_version | cut -d. -f1)
    local minor=$(echo $zsh_version | cut -d. -f2)
    if [[ $major -ge 5 ]] && [[ $minor -ge 0 ]]; then
        print_result pass "ZSH ${zsh_version}"
    else
        print_result warn "ZSH ${zsh_version}" "Recommended: 5.0+"
    fi
else
    print_result fail "Unable to detect ZSH version"
fi

# Check terminal information
if [[ -n "$TERM_PROGRAM" ]]; then
    if [[ -n "$TERM_PROGRAM_VERSION" ]]; then
        print_result pass "Terminal: ${TERM_PROGRAM} ${TERM_PROGRAM_VERSION}"
    else
        print_result pass "Terminal: ${TERM_PROGRAM}"
    fi
elif [[ -n "$TERM" ]]; then
    print_result pass "Terminal: ${TERM}"
else
    print_result info "Terminal: unknown"
fi

# 2. Check if forge is installed and in PATH
print_section "Forge Installation"

# Check FORGE_BIN environment variable
if [[ -n "$FORGE_BIN" ]]; then
    if [[ ! -e "$FORGE_BIN" ]]; then
        print_result fail "FORGE_BIN path does not exist: ${FORGE_BIN}"
    elif [[ ! -f "$FORGE_BIN" ]]; then
        print_result fail "FORGE_BIN is not a file: ${FORGE_BIN}"
    elif [[ ! -x "$FORGE_BIN" ]]; then
        print_result fail "FORGE_BIN is not executable: ${FORGE_BIN}"
    else
        print_result pass "FORGE_BIN: ${FORGE_BIN}"
    fi
else
    print_result warn "FORGE_BIN not set" "export FORGE_BIN=\$(which forge)"
fi

# Check if forge is in PATH
if command -v forge &> /dev/null; then
    local forge_path=$(command -v forge)
    
    # Get forge version and extract just the version number
    local forge_version=$(forge --version 2>&1 | head -n1 | awk '{print $2}')
    if [[ -n "$forge_version" ]]; then
        print_result pass "Forge: ${forge_version}"
        print_result info "${forge_path}"
    else
        print_result pass "Forge binary found: ${forge_path}"
    fi
else
    print_result fail "Forge binary not found in PATH" "Install from: https://github.com/your-org/forge"
fi

# 3. Check shell plugin and completions
print_section "Plugin & Completions"

# Check if forge plugin is loaded by checking environment variable
if [[ -n "$_FORGE_PLUGIN_LOADED" ]]; then
    print_result pass "Forge plugin loaded"
else
    print_result fail "Forge plugin not loaded"
    print_result info "Add to your ~/.zshrc:"
    print_result info "  eval \"\$(\$FORGE_BIN zsh plugin)\""
fi

# Check if completions are available
if (( $+functions[_forge] )); then
    print_result pass "Forge completions available"
else
    if [[ -n "$_FORGE_PLUGIN_LOADED" ]]; then
        print_result warn "Completions may not be properly initialized"
        print_result info "Ensure 'compinit' is called after loading the plugin"
    else
        print_result fail "Forge completions not loaded"
        print_result info "Load the plugin first (see above)"
    fi
fi

# 4. Check theme
print_section "ZSH Theme"

# Check if forge theme is loaded by checking environment variable
if [[ -n "$_FORGE_THEME_LOADED" ]]; then
    print_result pass "Forge theme loaded"
elif (( $+functions[p10k] )); then
    print_result info "Powerlevel10k detected (not using Forge theme)"
elif [[ -n "$ZSH_THEME" ]]; then
    print_result info "Using theme: ${ZSH_THEME}"
    print_result info "To use Forge theme, add to ~/.zshrc:"
    print_result info "  eval \"\$(\$FORGE_BIN zsh theme)\""
else
    print_result warn "No theme loaded"
    print_result info "To use Forge theme, add to ~/.zshrc:"
    print_result info "  eval \"\$(\$FORGE_BIN zsh theme)\""
fi

# 4. Check for common issues
print_section "System"

# Check editor configuration (FORGE_EDITOR takes precedence over EDITOR)
if [[ -n "$FORGE_EDITOR" ]]; then
    print_result pass "FORGE_EDITOR: ${FORGE_EDITOR}"
    if [[ -n "$EDITOR" ]]; then
        print_result info "EDITOR also set: ${EDITOR} (ignored)"
    fi
elif [[ -n "$EDITOR" ]]; then
    print_result pass "EDITOR: ${EDITOR}"
    print_result info "Tip: Set FORGE_EDITOR for forge-specific editor"
else
    print_result warn "No editor configured" "export EDITOR=vim or export FORGE_EDITOR=vim"
fi

# Check PATH for common issues
if [[ "$PATH" == *"/usr/local/bin"* ]]; then
    print_result pass "PATH configured"
else
    print_result warn "PATH missing /usr/local/bin"
fi

# 10. Check recommended ZSH plugins
print_section "Recommended Plugins"

# Check for zsh-autosuggestions
if [[ " ${plugins[*]} " =~ " zsh-autosuggestions " ]] || \
   [[ -n "$fpath[(r)*zsh-autosuggestions*]" ]] || \
   (( $+functions[_zsh_autosuggest_accept] )); then
    print_result pass "zsh-autosuggestions installed"
else
    print_result warn "zsh-autosuggestions not found"
    print_result info "Install plugin and add to plugins=() in .zshrc"
    print_result info "Installation guide: https://github.com/zsh-users/zsh-autosuggestions/blob/master/INSTALL.md"
fi

# Check for zsh-syntax-highlighting
if [[ " ${plugins[*]} " =~ " zsh-syntax-highlighting " ]] || \
   [[ -n "$fpath[(r)*zsh-syntax-highlighting*]" ]] || \
   (( $+functions[_zsh_highlight] )); then
    print_result pass "zsh-syntax-highlighting installed"
else
    print_result warn "zsh-syntax-highlighting not found"
    print_result info "Install plugin and add to plugins=() in .zshrc"
    print_result info "Installation guide: https://github.com/zsh-users/zsh-syntax-highlighting/blob/master/INSTALL.md"
fi

# 5. Check dependencies
print_section "Dependencies"

# Check for fzf - required for interactive selection
if command -v fzf &> /dev/null; then
    local fzf_version=$(fzf --version 2>&1 | head -n1 | awk '{print $1}')
    print_result pass "fzf: ${fzf_version}"
else
    print_result fail "fzf not found" "Required for interactive features: brew install fzf"
fi

# Check for fd/fdfind - used for file discovery
if command -v fd &> /dev/null; then
    local fd_version=$(fd --version 2>&1 | awk '{print $2}')
    print_result pass "fd: ${fd_version}"
elif command -v fdfind &> /dev/null; then
    local fd_version=$(fdfind --version 2>&1 | awk '{print $2}')
    print_result pass "fdfind: ${fd_version}"
else
    print_result warn "fd/fdfind not found" "Enhanced file discovery: brew install fd"
fi

# Check for bat - used for syntax highlighting
if command -v bat &> /dev/null; then
    local bat_version=$(bat --version 2>&1 | awk '{print $2}')
    print_result pass "bat: ${bat_version}"
else
    print_result warn "bat not found" "Enhanced preview: brew install bat"
fi

# Check font and Nerd Font support
# Show actual icons used in Forge theme
echo ""
echo "$(bold "Font Check [Manual Verification Required]")"
echo "  $(cyan "")  ${_DIM} configured via \$FORGE_FOLDER_ICON${RESET}"
echo "  $(cyan "")  ${_DIM} configured via \$FORGE_GIT_ICON${RESET}"
echo "  $(cyan "")  ${_DIM} configured via \$FORGE_MODEL_ICON${RESET}"
echo "  $(cyan "󱙺")  ${_DIM} configured via \$FORGE_AGENT_ICON${RESET}"
echo "  $(cyan "")  ${_DIM} configured via \$FORGE_PROMPT_SYMBOL${RESET}"
echo ""
echo "  Forge uses Nerd Fonts to enrich cli experience, can you see all 5 icons clearly?"
echo "  If you see boxes (□) or question marks (?), install a Nerd Font from:"
echo "  $(dim "https://www.nerdfonts.com/")"
echo ""

# Summary
echo ""
echo "$(dim "────────────────────────────────────────")"

if [[ $failed -eq 0 && $warnings -eq 0 ]]; then
    echo "$(green "${PASS}") $(bold "All checks passed") $(dim "(${passed})")"
    exit 0
elif [[ $failed -eq 0 ]]; then
    echo "$(yellow "${WARN}") $(bold "${warnings} warnings") $(dim "(${passed} passed)")"
    exit 0
else
    echo "$(red "${FAIL}") $(bold "${failed} failed") $(dim "(${warnings} warnings, ${passed} passed)")"
    exit 1
fi
