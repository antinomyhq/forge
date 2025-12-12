#!/usr/bin/env zsh

# ZSH Doctor - Diagnostic tool for Forge shell environment
# Checks for common configuration issues and environment setup

# Color codes for output
local RED='\033[0;31m'
local GREEN='\033[0;32m'
local YELLOW='\033[1;33m'
local BLUE='\033[0;34m'
local NC='\033[0m' # No Color

# Symbols
local CHECK="✓"
local CROSS="✗"
local WARNING="⚠"
local INFO="ℹ"

# Counters
local passed=0
local failed=0
local warnings=0

# Helper function to print results
function print_result() {
    local result_status=$1
    local message=$2
    local detail=$3
    
    case $result_status in
        pass)
            echo "${GREEN}${CHECK}${NC} ${message}"
            ((passed++))
            ;;
        fail)
            echo "${RED}${CROSS}${NC} ${message}"
            [[ -n "$detail" ]] && echo "  ${detail}"
            ((failed++))
            ;;
        warn)
            echo "${YELLOW}${WARNING}${NC} ${message}"
            [[ -n "$detail" ]] && echo "  ${detail}"
            ((warnings++))
            ;;
        info)
            echo "${BLUE}${INFO}${NC} ${message}"
            ;;
    esac
}

echo "╔════════════════════════════════════════════════════════════╗"
echo "║          Forge ZSH Environment Diagnostics                 ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# 1. Check ZSH version
echo "🔍 Shell Environment"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
local zsh_version="${ZSH_VERSION}"
if [[ -n "$zsh_version" ]]; then
    local major=$(echo $zsh_version | cut -d. -f1)
    local minor=$(echo $zsh_version | cut -d. -f2)
    if [[ $major -ge 5 ]] && [[ $minor -ge 0 ]]; then
        print_result pass "ZSH version: ${zsh_version}"
    else
        print_result warn "ZSH version: ${zsh_version}" "Recommended: 5.0 or higher"
    fi
else
    print_result fail "Unable to detect ZSH version"
fi

# 2. Check if forge is installed and in PATH
echo ""
echo "🔧 Forge Installation"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
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
echo ""
echo "🔌 Forge Plugin Status"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

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
echo ""
echo "🌍 Environment Variables"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [[ -n "$FORGE_CONVERSATION_ID" ]]; then
    print_result info "Active conversation: ${FORGE_CONVERSATION_ID}"
else
    print_result info "No active conversation"
fi

if [[ -n "$FORGE_AGENT" ]]; then
    print_result info "Current agent: ${FORGE_AGENT}"
else
    print_result info "Using default agent"
fi

if [[ -n "$FORGE_MODEL" ]]; then
    print_result info "Current model: ${FORGE_MODEL}"
else
    print_result info "Using default model"
fi

if [[ -n "$FORGE_PROVIDER" ]]; then
    print_result info "Current provider: ${FORGE_PROVIDER}"
else
    print_result info "Using default provider"
fi

# 5. Check completion system
echo ""
echo "📝 Completion System"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

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
echo ""
echo "⌨️  Key Bindings"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

local widget=$(bindkey | grep "forge-accept-line" | head -n1)
if [[ -n "$widget" ]]; then
    print_result pass "Forge accept-line widget bound"
    print_result info "$(echo $widget | awk '{print $1 " -> " $2}')"
else
    print_result warn "Forge accept-line widget not bound" "Check plugin initialization"
fi

# 7. Check theme
echo ""
echo "🎨 Theme Status"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if (( $+functions[_update_forge_vars] )); then
    print_result pass "Forge theme functions loaded"
else
    print_result info "Forge theme not loaded (optional)"
fi

if (( $+functions[p10k] )); then
    print_result info "Powerlevel10k detected"
elif [[ -n "$ZSH_THEME" ]]; then
    print_result info "Theme: ${ZSH_THEME}"
else
    print_result info "No Oh-My-Zsh theme detected"
fi

# 8. Check config file
echo ""
echo "⚙️  Configuration"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

local config_file="${HOME}/.config/forge/config.yaml"
if [[ -f "$config_file" ]]; then
    print_result pass "Config file exists: ${config_file}"
    
    # Check if file is readable
    if [[ -r "$config_file" ]]; then
        print_result pass "Config file is readable"
    else
        print_result fail "Config file is not readable"
    fi
else
    print_result warn "Config file not found: ${config_file}" "Will use default configuration"
fi

# 9. Check for common issues
echo ""
echo "🔎 Common Issues"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Check if EDITOR is set
if [[ -n "$EDITOR" ]]; then
    print_result pass "EDITOR is set: ${EDITOR}"
else
    print_result warn "EDITOR is not set" "Some features may not work. Set it in .zshrc: export EDITOR=vim"
fi

# Check PATH for common issues
if [[ "$PATH" == *"/usr/local/bin"* ]]; then
    print_result pass "PATH includes /usr/local/bin"
else
    print_result warn "PATH does not include /usr/local/bin"
fi

# Check for conflicting plugins
local conflicting_plugins=(
    "zsh-autosuggestions"
    "zsh-syntax-highlighting"
)

for plugin in $conflicting_plugins; do
    if [[ -n "$fpath[(r)*${plugin}*]" ]] || (( $+functions[${plugin}] )); then
        print_result info "Plugin detected: ${plugin} (may affect Forge)"
    fi
done

# 10. Network connectivity (basic check)
echo ""
echo "🌐 Network"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if command -v curl &> /dev/null; then
    print_result pass "curl is available"
else
    print_result warn "curl not found" "May affect some Forge features"
fi

# Summary
echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║                        Summary                             ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "${GREEN}Passed:${NC}   ${passed}"
echo "${RED}Failed:${NC}   ${failed}"
echo "${YELLOW}Warnings:${NC} ${warnings}"
echo ""

if [[ $failed -eq 0 ]]; then
    echo "${GREEN}${CHECK} All critical checks passed!${NC}"
    exit 0
else
    echo "${RED}${CROSS} Some critical checks failed. Please review the issues above.${NC}"
    exit 1
fi
