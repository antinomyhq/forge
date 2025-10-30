---
name: setup-terminal
description: Automates the ZSH integration setup from docs
---

Automate the 5-step ZSH integration process from https://forgecode.dev/docs/zsh-support/

**Step 1: Install zsh-syntax-highlighting (Required)**
- Try package manager: `brew`/`apt`/`pacman`/`dnf install zsh-syntax-highlighting`
- Or Oh My Zsh:
  ```bash
  git clone https://github.com/zsh-users/zsh-syntax-highlighting.git ${ZSH_CUSTOM:-~/.oh-my-zsh/custom}/plugins/zsh-syntax-highlighting
  ```
  Then add to plugins in ~/.zshrc: `plugins=(... zsh-syntax-highlighting)` (must be last plugin)
- Or Manual:
  ```bash
  git clone https://github.com/zsh-users/zsh-syntax-highlighting.git
  echo "source ${(q-)PWD}/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh" >> ${ZDOTDIR:-$HOME}/.zshrc
  ```
- Must be sourced LAST in ~/.zshrc

**Step 2: Install fd and fzf (Optional but Recommended)**
- Package manager: `brew install fd fzf` or `apt install fd-find fzf` or `pacman -S fd fzf`
- Note: Ubuntu/Debian uses `fd-find` package name
- Skip if installation fails - user can add later

**Step 3: Configure ZSH Integration**
Detect Forge installation type: Use `which forge` or `command -v forge` to check if installed globally, otherwise use `npx forgecode@latest`

Backup ~/.zshrc first, then add to it:
```bash
# If Forge is global
export FORGE_BIN=forge
source <($FORGE_BIN extension zsh)

# OR if using npx
export FORGE_BIN="npx forgecode@latest"
source <($FORGE_BIN extension zsh)
```
Remove any existing FORGE_BIN exports or Forge integration lines to avoid duplicates.

**Critical Loading Order:**
1. All other plugins
2. Forge integration (above lines)
3. zsh-syntax-highlighting source (MUST BE LAST)

**Step 4: Verify Installation**
Reload shell: `source ~/.zshrc` or `exec zsh`

Run verification tests:
1. **Basic integration**: `:forge` (should execute without "command not found" error)
2. **Syntax highlighting**: Check if `$ZSH_HIGHLIGHT_VERSION` is set (indicates zsh-syntax-highlighting loaded)
3. **fd/fzf integration** (if installed in Step 2):
   - Check `which fd` returns a path
   - Check `which fzf` returns a path
   - If both exist, fuzzy finding features will work

Report any test failures for troubleshooting.

**Report**: Show what was installed, backup location (if created).

**Auto-heal**: If any step fails, consult the Troubleshooting section at https://forgecode.dev/docs/zsh-support/, diagnose the specific issue, and apply the documented fixes.
