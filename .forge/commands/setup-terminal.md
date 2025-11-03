---
name: setup-terminal
description: Automates the ZSH integration setup from docs
---

Your Goal is to automate ZSH integration for Forge. Reference: https://forgecode.dev/docs/zsh-support/

**Rules:** No bash scripts. Prefer package managers. Backup ~/.zshrc. Loading order: plugins → Forge → zsh-syntax-highlighting (LAST).

## Step 1: Detect OS and Package Manager

Detect OS and package manager commands:
- macOS: `brew install`
- Ubuntu/Debian: `sudo apt install`
- Arch: `sudo pacman -S`
- Fedora: `sudo dnf install`

## Step 2: Install Dependencies

Use detected package manager to install:
- zsh-syntax-highlighting
- fd (note: use `fd-find` on Ubuntu/Debian)
- fzf

## Step 3: Configure ZSH

1. Detect Forge: `which forge` or `command -v forge` or use command depending on OS.
2. Backup: `cp ~/.zshrc ~/.zshrc.backup.$(date +%Y%m%d_%H%M%S)`
3. Remove existing `FORGE_BIN` exports and Forge lines from ~/.zshrc
4. Add to ~/.zshrc:
   - Global: `export FORGE_BIN=forge` and `source <($FORGE_BIN extension zsh)`
   - npx: `export FORGE_BIN="npx forgecode@latest"` and `source <($FORGE_BIN extension zsh)`
5. Ensure zsh-syntax-highlighting loads LAST if installed.

## Step 4: Verify

1. Reload: `source ~/.zshrc`
2. Run: `zsh -i -c 'forge_verify_dependencies'`
3. If fails, check https://forgecode.dev/docs/zsh-support/ troubleshooting

**Report:** installations, backup location, verification results, issues resolved.
