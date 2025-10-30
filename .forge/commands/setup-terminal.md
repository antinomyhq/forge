---
name: setup-terminal
description: Automatically configures ZSH integration with Forge
---

Automate ZSH integration for Forge. Fix issues automatically.

## Step 1: Detect Environment

- Verify ZSH is installed and set as default shell
- Detect Forge method: check if `forge` is global, else use `FORGE_BIN="npx forgecode"`
- Identify OS and package manager (brew/apt/pacman/dnf)

## Step 2: Install Dependencies

- Backup `.zshrc` to `~/.zshrc.backup.$(date +%Y%m%d_%H%M%S)`
- Install zsh-syntax-highlighting via package manager
  - If fails: update package cache and retry
- Install fd and fzf (optional but recommended)
  - If fails: try alternative package names (`fd-find`)
  - Create symlinks if needed
- On failures: fetch https://forgecode.dev/docs/zsh-support/ for current fixes

## Step 3: Configure .zshrc

- Remove any existing Forge integration lines
- Move zsh-syntax-highlighting to end if present
- Add before any syntax highlighting:
  ```bash
  export FORGE_BIN=<detected-method>
  source <($FORGE_BIN extension zsh)
  ```
- Ensure zsh-syntax-highlighting is sourced last
- Verify paths exist before adding

## Step 4: Verify

- Source `.zshrc` and check for errors
  - If sourcing fails: parse error, fix issue (paths/permissions/syntax), retry
- Test: FORGE_BIN set, extension loaded, syntax highlighting active
  - If test fails: fix the specific issue and re-source
- On errors: fetch docs, apply fixes, restore backup if needed

## Step 5: Report

Show:
- What was installed
- Backup location
- Quick examples:
  ```bash
  : hello                    # Basic prompt
  :reset                     # Clear context
  ```
- Link: https://forgecode.dev/docs/zsh-support/

If all fails: rollback, show diagnostics, link to troubleshooting.
