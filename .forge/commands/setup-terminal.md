---
name: setup-terminal
description: Automates the ZSH integration setup from docs
---

You are tasked with automating the ZSH integration setup process for Forge. Follow the steps below exactly. Reference documentation at https://forgecode.dev/docs/zsh-support/ if needed.

## Critical Requirements

- DO NOT create bash scripts or intermediate files
- Check for package manager installations FIRST - they are simpler and easier to maintain
- Always backup ~/.zshrc before making changes
- Maintain proper loading order in ~/.zshrc (plugins → Forge → zsh-syntax-highlighting LAST)

## Step 1: Install zsh-syntax-highlighting

Check and install in this priority order:

1. **Package Manager (PREFERRED)**: Detect the operating system and use the appropriate package manager:
   - macOS: Run `brew install zsh-syntax-highlighting`
   - Ubuntu/Debian: Run `sudo apt install zsh-syntax-highlighting`
   - Arch Linux: Run `sudo pacman -S zsh-syntax-highlighting`
   - Fedora: Run `sudo dnf install zsh-syntax-highlighting`

2. **Oh My Zsh Plugin (if package manager unavailable)**: 
   - Clone repository: `git clone https://github.com/zsh-users/zsh-syntax-highlighting.git ${ZSH_CUSTOM:-~/.oh-my-zsh/custom}/plugins/zsh-syntax-highlighting`
   - Add `zsh-syntax-highlighting` to the plugins array in ~/.zshrc (must be the last plugin in the array)

3. **Manual Installation (last resort)**:
   - Clone repository: `git clone https://github.com/zsh-users/zsh-syntax-highlighting.git`
   - Add sourcing line to ~/.zshrc: `source ${(q-)PWD}/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh`

Note: Package manager installations handle sourcing automatically. For Oh My Zsh or manual installations, ensure zsh-syntax-highlighting loads LAST in ~/.zshrc.

## Step 2: Install fd and fzf

Install using the appropriate package manager:
- macOS: Run `brew install fd fzf`
- Ubuntu/Debian: Run `sudo apt install fd-find fzf` (note: package is named `fd-find` on Debian-based systems)
- Arch Linux: Run `sudo pacman -S fd fzf`
- Fedora: Run `sudo dnf install fd fzf`

## Step 3: Configure ZSH Integration

1. **Detect Forge installation type**: Run `which forge` or `command -v forge` to determine if Forge is globally installed or needs to be run via npx

2. **Backup ~/.zshrc**: Create a backup with timestamp: `cp ~/.zshrc ~/.zshrc.backup.$(date +%Y%m%d_%H%M%S)`

3. **Remove existing Forge configuration**: Search for and remove any existing `FORGE_BIN` exports or Forge integration lines from ~/.zshrc to prevent duplicates

4. **Add Forge integration**: Append the appropriate configuration to ~/.zshrc:
   - If Forge is globally installed:
     ```
     export FORGE_BIN=forge
     source <($FORGE_BIN extension zsh)
     ```
   - If using npx:
     ```
     export FORGE_BIN="npx forgecode@latest"
     source <($FORGE_BIN extension zsh)
     ```

5. **Ensure proper loading order in ~/.zshrc**:
   - All other plugins and configurations first
   - Forge integration (lines added above)
   - zsh-syntax-highlighting source line MUST BE LAST

## Step 4: Reload Shell and Verify

1. **Reload the shell**: Run `source ~/.zshrc` or `exec zsh` to apply changes

2. **Run verification**: Execute the built-in verification function:
   ```
   zsh -i -c 'forge_verify_dependencies'
   ```

3. **Check verification results**: The function verifies:
   - ZLE Widgets Registration (forge-accept-line and forge-completion)
   - zsh-syntax-highlighting installation and version
   - fd/fzf integration

4. **Report any failures**: If verification fails, consult the Troubleshooting section at https://forgecode.dev/docs/zsh-support/ to diagnose and fix the issue

## Final Report

Provide a summary including:
- What was installed (packages, plugins, or manual installations)
- Backup file location (if created)
- Verification results
- Any issues encountered and how they were resolved

If any step fails, automatically diagnose the issue using the troubleshooting documentation and apply the documented fixes.
