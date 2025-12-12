#!/usr/bin/env zsh

# ZSH Doctor - Diagnostic tool for Forge shell environment
# Checks for common configuration issues and environment setup

# ANSI codes
local RESET='\033[0m'
local _BOLD='\033[1m'
local _DIM='\033[2m'
local _GREEN='\033[0;32m'
local _RED='\033[0;31m'
local _YELLOW='\033[0;33m'

# Text formatting helpers - auto-reset
function bold() { echo "${_BOLD}${1}${RESET}"; }
function dim() { echo "${_DIM}${1}${RESET}"; }
function green() { echo "${_GREEN}${1}${RESET}"; }
function red() { echo "${_RED}${1}${RESET}"; }
function yellow() { echo "${_YELLOW}${1}${RESET}"; }

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

echo "$(bold "Forge Environment Diagnostics")"

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

# 2. Check if forge is installed and in PATH
print_section "Forge Installation"

# Check FORGE_BIN environment variable
if [[ -n "$FORGE_BIN" ]]; then
    if [[ -f "$FORGE_BIN" && -x "$FORGE_BIN" ]]; then
        print_result pass "FORGE_BIN set: ${FORGE_BIN}"
    else
        print_result fail "FORGE_BIN set but not executable: ${FORGE_BIN}"
    fi
else
    print_result info "FORGE_BIN not set"
fi

# Check if forge is in PATH
if command -v forge &> /dev/null; then
    local forge_path=$(command -v forge)
    print_result pass "Forge binary found: ${forge_path}"
    
    # Get forge version
    local forge_version=$(forge --version 2>&1 | head -n1 || echo "unknown")
    print_result info "Version: ${forge_version}"
else
    print_result fail "Forge binary not found in PATH" "Install from: https://github.com/your-org/forge"
fi

# 3. Check forge plugin loading
print_section "Plugin Status"

# Check if forge-accept-line function exists (indicates plugin is loaded)
if (( $+functions[forge-accept-line] )); then
    print_result pass "Forge plugin is loaded"
else
    print_result fail "Forge plugin is not loaded" "Add to .zshrc: source <(forge zsh plugin)"
fi

# Check if _forge_action_default exists
if (( $+functions[_forge_action_default] )); then
    print_result pass "Forge dispatcher is loaded"
else
    print_result warn "Forge dispatcher not found"
fi

# 4. Check environment variables
print_section "Environment"

if [[ -n "$FORGE_CONVERSATION_ID" ]]; then
    print_result info "Conversation: ${FORGE_CONVERSATION_ID}"
else
    print_result info "Conversation: none"
fi

if [[ -n "$FORGE_AGENT" ]]; then
    print_result info "Agent: ${FORGE_AGENT}"
fi

if [[ -n "$FORGE_MODEL" ]]; then
    print_result info "Model: ${FORGE_MODEL}"
fi

if [[ -n "$FORGE_PROVIDER" ]]; then
    print_result info "Provider: ${FORGE_PROVIDER}"
fi

# 5. Check completion system
print_section "Completion System"

if (( $+functions[compinit] )); then
    print_result pass "ZSH completion system initialized"
else
    print_result warn "ZSH completion system not initialized" "Add to .zshrc: autoload -Uz compinit && compinit"
fi

# Check if forge completion is available
if (( $+functions[_forge] )); then
    print_result pass "Forge completion function loaded"
else
    print_result warn "Forge completion not loaded" "Ensure plugin is sourced before compinit"
fi

# 6. Check key bindings
print_section "Key Bindings"

local widget=$(bindkey | grep "forge-accept-line" | head -n1)
if [[ -n "$widget" ]]; then
    print_result pass "Accept-line widget bound"
    print_result info "$(echo $widget | awk '{print $1}')"
else
    print_result warn "Accept-line widget not bound" "Check plugin initialization"
fi

# 7. Check theme
print_section "Theme"

if (( $+functions[_update_forge_vars] )); then
    print_result pass "Forge theme loaded"
elif (( $+functions[p10k] )); then
    print_result info "Powerlevel10k detected"
elif [[ -n "$ZSH_THEME" ]]; then
    print_result info "${ZSH_THEME}"
else
    print_result info "Default theme"
fi

# 8. Check config file
print_section "Configuration"

local config_file="${HOME}/.config/forge/config.yaml"
if [[ -f "$config_file" ]]; then
    if [[ -r "$config_file" ]]; then
        print_result pass "Config file readable"
        print_result info "${config_file}"
    else
        print_result fail "Config file not readable"
        print_result info "${config_file}"
    fi
else
    print_result warn "Config file not found" "Using defaults"
fi

# 9. Check for common issues
print_section "System"

# Check if EDITOR is set
if [[ -n "$EDITOR" ]]; then
    print_result pass "EDITOR: ${EDITOR}"
else
    print_result warn "EDITOR not set" "export EDITOR=vim"
fi

# Check PATH for common issues
if [[ "$PATH" == *"/usr/local/bin"* ]]; then
    print_result pass "PATH configured"
else
    print_result warn "PATH missing /usr/local/bin"
fi

# curl check
if command -v curl &> /dev/null; then
    print_result pass "curl available"
else
    print_result warn "curl not found" "May affect some features"
fi

# Check for notable plugins
local notable_plugins=(
    "zsh-autosuggestions"
    "zsh-syntax-highlighting"
)

for plugin in $notable_plugins; do
    if [[ -n "$fpath[(r)*${plugin}*]" ]] || (( $+functions[${plugin}] )); then
        print_result info "Plugin: ${plugin}"
    fi
done

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
