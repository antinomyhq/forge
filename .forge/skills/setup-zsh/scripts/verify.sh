#!/bin/bash
# Verify ZSH plugin setup and dependencies
# Returns machine-readable output with all status information

set -euo pipefail

# Check if a command exists in PATH
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Detect the package manager
detect_package_manager() {
    if command_exists brew; then
        echo "brew"
    elif command_exists apt; then
        echo "apt"
    elif command_exists pacman; then
        echo "pacman"
    else
        echo "unknown"
    fi
}

# Detect the ZSH framework
detect_framework() {
    local home="$1"
    if [ -d "$home/.oh-my-zsh" ]; then
        echo "oh-my-zsh"
    elif [ -d "$home/.zprezto" ]; then
        echo "prezto"
    else
        echo "standalone"
    fi
}

# Get the syntax highlighting directory based on framework
get_syntax_dir() {
    local framework="$1"
    local home="$2"
    
    case "$framework" in
        oh-my-zsh)
            local zsh_custom="${ZSH_CUSTOM:-$home/.oh-my-zsh/custom}"
            echo "$zsh_custom/plugins/zsh-syntax-highlighting"
            ;;
        standalone)
            echo "$home/.zsh/zsh-syntax-highlighting"
            ;;
        *)
            echo ""
            ;;
    esac
}

# Main verification logic
main() {
    # 1. Check OS
    local os
    case "$(uname -s)" in
        Darwin) os="macos" ;;
        Linux) os="linux" ;;
        *) os="unknown" ;;
    esac
    
    # 2. Check package manager
    local package_manager
    package_manager=$(detect_package_manager)
    
    # 3. Check dependencies
    local fzf_installed="false"
    local fd_installed="false"
    local bat_installed="false"
    local forge_installed="false"
    
    command_exists fzf && fzf_installed="true"
    (command_exists fd || command_exists fdfind) && fd_installed="true"
    command_exists bat && bat_installed="true"
    command_exists forge && forge_installed="true"
    
    # 4. Detect ZSH framework
    local home="${HOME:-$HOME}"
    local framework
    framework=$(detect_framework "$home")
    
    # 5. Check zsh-syntax-highlighting installation
    local syntax_dir
    syntax_dir=$(get_syntax_dir "$framework" "$home")
    local syntax_highlighting_installed="false"
    [ -n "$syntax_dir" ] && [ -d "$syntax_dir" ] && syntax_highlighting_installed="true"
    
    # 6. Check if syntax-highlighting is configured in zshrc
    local zshrc_path="$home/.zshrc"
    local syntax_highlighting_configured="false"
    if [ -f "$zshrc_path" ]; then
        grep -q "zsh-syntax-highlighting" "$zshrc_path" && syntax_highlighting_configured="true"
    fi
    
    # 7. Check if Forge plugin is sourced
    local forge_plugin_sourced="false"
    if [ -f "$zshrc_path" ]; then
        if grep -q "forge" "$zshrc_path" && \
           grep -q "extension" "$zshrc_path" && \
           grep -q "zsh" "$zshrc_path"; then
            forge_plugin_sourced="true"
        fi
    fi
    
    # 8. Check if FORGE_BIN is set
    local forge_bin_set="false"
    if [ -f "$zshrc_path" ]; then
        grep -q "export FORGE_BIN=" "$zshrc_path" && forge_bin_set="true"
    fi
    
    # 9. Determine if setup is complete
    local setup_complete="false"
    if [ "$fzf_installed" = "true" ] && \
       [ "$fd_installed" = "true" ] && \
       [ "$forge_installed" = "true" ] && \
       [ "$syntax_highlighting_configured" = "true" ] && \
       [ "$forge_plugin_sourced" = "true" ]; then
        setup_complete="true"
    fi
    
    # Output in tab-separated format for easy parsing
    cat <<EOF
Status	$([ "$setup_complete" = "true" ] && echo "✓ Complete" || echo "✗ Incomplete")
OS	$os
Package Manager	$package_manager
Framework	$framework
fzf	$([ "$fzf_installed" = "true" ] && echo "✓ Installed" || echo "✗ Missing")
fd	$([ "$fd_installed" = "true" ] && echo "✓ Installed" || echo "✗ Missing")
bat	$([ "$bat_installed" = "true" ] && echo "✓ Installed" || echo "✗ Missing")
forge	$([ "$forge_installed" = "true" ] && echo "✓ Installed" || echo "✗ Missing")
Syntax Highlighting Installed	$([ "$syntax_highlighting_installed" = "true" ] && echo "✓ Yes" || echo "✗ No")
Syntax Highlighting Configured	$([ "$syntax_highlighting_configured" = "true" ] && echo "✓ Yes" || echo "✗ No")
Forge Plugin Sourced	$([ "$forge_plugin_sourced" = "true" ] && echo "✓ Yes" || echo "✗ No")
FORGE_BIN Set	$([ "$forge_bin_set" = "true" ] && echo "✓ Yes" || echo "✗ No")
EOF
}

main "$@"
